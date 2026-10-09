//! Retained absolute-path directory capabilities and their parent-child identities.

use std::{
    ffi::OsString,
    fs::File,
    io,
    path::{Component, Path},
};

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::{
    ambient_authority,
    fs::{Dir, Metadata, OpenOptions},
};

use super::platform;

pub(super) struct PathBinding {
    directories: Vec<Dir>,
    identities: Vec<(u64, u64)>,
    names: Vec<OsString>,
    final_name: OsString,
}

impl PathBinding {
    pub(super) fn open(path: &Path) -> io::Result<(Self, File)> {
        let (anchor, names) = absolute(path)?;
        let (final_name, ancestors) = names.split_last().ok_or_else(platform::invalid)?;
        let mut directories = vec![anchor];
        let mut identities = vec![directory_identity(&directories[0])?];
        for name in ancestors {
            let parent = directories.last().ok_or_else(platform::invalid)?;
            let mut options = options();
            platform::directory_options(&mut options);
            let child = Dir::from_std_file(parent.open_with(name, &options)?.into_std());
            identities.push(directory_identity(&child)?);
            directories.push(child);
        }
        let parent = directories.last().ok_or_else(platform::invalid)?;
        let mut options = options();
        platform::file_options(&mut options);
        let file = parent.open_with(final_name, &options)?.into_std();
        let binding = Self {
            directories,
            identities,
            names: ancestors.to_vec(),
            final_name: final_name.clone(),
        };
        binding.revalidate(&Metadata::from_file(&file)?)?;
        Ok((binding, file))
    }

    pub(super) fn revalidate(&self, file: &Metadata) -> io::Result<()> {
        for (index, directory) in self.directories.iter().enumerate() {
            if directory_identity(directory)? != self.identities[index] {
                return Err(platform::invalid());
            }
            if index != 0 {
                let parent = &self.directories[index - 1];
                let metadata = parent.symlink_metadata(&self.names[index - 1])?;
                if !metadata.is_dir()
                    || platform::linked(&metadata)
                    || platform::identity(&metadata)? != self.identities[index]
                {
                    return Err(platform::invalid());
                }
            }
        }
        let parent = self.directories.last().ok_or_else(platform::invalid)?;
        platform::final_binding(parent, &self.final_name, file)
    }
}

fn directory_identity(directory: &Dir) -> io::Result<(u64, u64)> {
    let metadata = directory.dir_metadata()?;
    if !metadata.is_dir() || platform::linked(&metadata) {
        return Err(platform::invalid());
    }
    platform::identity(&metadata)
}

fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    options
}

#[cfg(unix)]
fn absolute(path: &Path) -> io::Result<(Dir, Vec<OsString>)> {
    let mut components = path.components();
    if components.next() != Some(Component::RootDir) {
        return Err(platform::invalid());
    }
    let names = names(components)?;
    Ok((Dir::open_ambient_dir("/", ambient_authority())?, names))
}

#[cfg(windows)]
fn absolute(path: &Path) -> io::Result<(Dir, Vec<OsString>)> {
    use std::path::{PathBuf, Prefix};
    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            _ => return Err(platform::invalid()),
        },
        _ => return Err(platform::invalid()),
    };
    if components.next() != Some(Component::RootDir) {
        return Err(platform::invalid());
    }
    let names = names(components)?;
    let root = PathBuf::from(format!("{}:\\", char::from(drive)));
    Ok((Dir::open_ambient_dir(root, ambient_authority())?, names))
}

#[cfg(not(any(unix, windows)))]
fn absolute(_: &Path) -> io::Result<(Dir, Vec<OsString>)> {
    Err(platform::invalid())
}

fn names<'a>(components: impl Iterator<Item = Component<'a>>) -> io::Result<Vec<OsString>> {
    components
        .map(|component| match component {
            Component::Normal(name) => Ok(name.to_os_string()),
            _ => Err(platform::invalid()),
        })
        .collect()
}
