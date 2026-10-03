//! Driver access to the shared process boundary used by frontend and architecture spawns.

#[cfg(unix)]
pub(crate) use zryna_process::spawn;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) use zryna_process::snapshot_writer;

#[cfg(test)]
pub(crate) use zryna_process::output;

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
pub(crate) use zryna_process::status;

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests;
