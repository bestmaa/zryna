//! Capability-relative private staging writes and their snapshot/spawn exclusion lifetime.

use std::{fs, io::Write as _, os::unix::fs::PermissionsExt as _, path::Path};

use cap_std::fs::OpenOptionsExt as _;
use zryna_diagnostics::Diagnostic;

use super::{NativeStage, stage_support::staging_write_error};

impl NativeStage {
    pub(super) fn write_input(&self, path: &Path, bytes: &[u8]) -> Result<(), Diagnostic> {
        self.revalidate()?;
        if path.parent() != Some(self.directory.as_path()) {
            return Err(staging_write_error());
        }
        let name = path.file_name().ok_or_else(staging_write_error)?;
        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        let _writer = crate::process_spawn::snapshot_writer().map_err(|_| staging_write_error())?;
        let mut file =
            self.directory_handle.open_with(name, &options).map_err(|_| staging_write_error())?;
        file.write_all(bytes)
            .and_then(|()| file.flush())
            .and_then(|()| file.sync_all())
            .map_err(|_| staging_write_error())?;
        file.set_permissions(cap_std::fs::Permissions::from_std(fs::Permissions::from_mode(0o600)))
            .map_err(|_| staging_write_error())?;
        drop(file);
        self.revalidate()
    }
}
