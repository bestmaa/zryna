//! Narrow Windows directory mutations and read-only explicit grant-file privacy proofs.
//!
//! The API creates a directory and acquires its authoritative capability in one
//! operation. That opaque capability can then perform a no-replace rename or an
//! exact empty-directory removal without selecting the source again by path.
//! Directory mutations expose no ambient-path operation or arbitrary source handle.
//! The grant proof separately observes one already-opened file without mutating its ACL.

#[cfg(windows)]
mod windows;

#[cfg(windows)]
mod private_grant;
#[cfg(any(windows, test))]
mod private_grant_policy;
#[cfg(windows)]
pub use private_grant::{PrivateGrantFile, admit_private_grant_file};

#[cfg(windows)]
pub use windows::{OwnedDirectory, create_directory};
