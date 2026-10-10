//! Same-handle reads and revalidation observe changes. The caller owning the file
//! can still change it between checks.

mod binding;
mod platform;

#[cfg(test)]
mod tests;

use std::{
    fs::File,
    io::{Read as _, Seek as _, SeekFrom},
    path::Path,
};

use cap_std::fs::Metadata;
use zryna_diagnostics::Diagnostic;

use super::{CommandRequest, MAX_REQUEST_BYTES, admit, rejection};
use binding::PathBinding;
use platform::PrivateFile;

/// Retains the original private file, captured bytes and admitted input together.
/// This proves captured input only; root approval and capability authority stay separate.
pub(crate) struct CapturedRequest {
    binding: PathBinding,
    file: PrivateFile,
    state: Metadata,
    bytes: Vec<u8>,
    request: CommandRequest,
}

impl CapturedRequest {
    pub(crate) fn capture(path: &Path, required_key: Option<&str>) -> Result<Self, Diagnostic> {
        Self::capture_impl(path, required_key, || Ok(()))
    }

    #[cfg(test)]
    fn capture_with_after_first_read(
        path: &Path,
        required_key: Option<&str>,
        after_read: impl FnOnce() -> std::io::Result<()>,
    ) -> Result<Self, Diagnostic> {
        Self::capture_impl(path, required_key, after_read)
    }

    fn capture_impl(
        path: &Path,
        required_key: Option<&str>,
        after_read: impl FnOnce() -> std::io::Result<()>,
    ) -> Result<Self, Diagnostic> {
        if required_key.is_none() {
            return Err(rejection());
        }
        let (binding, original) = PathBinding::open(path).map_err(|_| rejection())?;
        let file = platform::retain(original).map_err(|_| rejection())?;
        let state = Metadata::from_file(file.file()).map_err(|_| rejection())?;
        platform::privacy(&file, &state).map_err(|_| rejection())?;
        binding.revalidate(&state).map_err(|_| rejection())?;
        let bytes = read(file.file())?;
        after_read().map_err(|_| rejection())?;
        validate(&binding, &file, &state)?;
        let second = read(file.file())?;
        validate(&binding, &file, &state)?;
        if bytes != second {
            return Err(rejection());
        }
        let request = admit(&bytes, required_key)?;
        Ok(Self { binding, file, state, bytes, request })
    }

    pub(crate) fn request(&self) -> &CommandRequest {
        &self.request
    }

    /// Rechecks the exact retained handle and path bindings without selecting new input.
    pub(crate) fn revalidate(&self) -> Result<(), Diagnostic> {
        validate(&self.binding, &self.file, &self.state)?;
        let current = read(self.file.file())?;
        validate(&self.binding, &self.file, &self.state)?;
        if current != self.bytes {
            return Err(rejection());
        }
        Ok(())
    }
}

fn validate(binding: &PathBinding, file: &PrivateFile, state: &Metadata) -> Result<(), Diagnostic> {
    let current = Metadata::from_file(file.file()).map_err(|_| rejection())?;
    platform::privacy(file, &current).map_err(|_| rejection())?;
    if !platform::same_state(state, &current) {
        return Err(rejection());
    }
    binding.revalidate(&current).map_err(|_| rejection())
}

fn read(mut file: &File) -> Result<Vec<u8>, Diagnostic> {
    let length = file.metadata().map_err(|_| rejection())?.len();
    if length > MAX_REQUEST_BYTES as u64 {
        return Err(rejection());
    }
    file.seek(SeekFrom::Start(0)).map_err(|_| rejection())?;
    let mut bytes = Vec::new();
    file.take(MAX_REQUEST_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| rejection())?;
    if bytes.len() > MAX_REQUEST_BYTES || bytes.len() as u64 != length {
        return Err(rejection());
    }
    Ok(bytes)
}
