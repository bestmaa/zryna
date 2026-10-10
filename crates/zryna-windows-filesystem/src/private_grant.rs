//! Exact-file privacy proof for one explicitly supplied host grant input.

use std::{fs::File, io};

use crate::{private_grant_policy, windows};

/// Owns one regular single-link grant file whose exact owner and DACL were verified.
///
/// This does not approve a capability, interpret the file, mutate permissions or reopen a path.
pub struct PrivateGrantFile {
    file: File,
    identity: (u32, u32, u32),
    descriptor: Vec<u8>,
    user: Vec<u8>,
}

impl PrivateGrantFile {
    /// Borrows the retained exact file for the driver's bounded immutable capture.
    #[must_use]
    pub const fn file(&self) -> &File {
        &self.file
    }

    /// Rechecks the same handle, caller identity, owner and complete DACL after capture.
    /// Observations detect changes; they do not exclude owner-authorized changes between checks.
    ///
    /// # Errors
    /// Rejects changed identity or policy and any failed security observation.
    pub fn revalidate(&self) -> io::Result<()> {
        let user = windows::effective_user_sid()?;
        let descriptor = windows::private_file_descriptor(&self.file)?;
        private_grant_policy::verify(&descriptor, &user)?;
        if windows::regular_file_identity(&self.file)? != self.identity
            || descriptor != self.descriptor
            || user != self.user
        {
            return Err(private_grant_policy::rejected());
        }
        Ok(())
    }
}

/// Admits ownership of an already opened explicit grant file through its exact handle.
///
/// The caller must request `READ_CONTROL` on the original handle, open through retained
/// no-reparse ancestors with write/delete sharing
/// excluded. The driver retains responsibility for the absolute path, byte limit, stable read,
/// source/grant intersection and disposal. Owner and `LocalSystem` are the only admitted
/// effective allow trustees; an Administrators or inherited broad allow is rejected.
///
/// # Errors
/// Rejects a directory, reparse point, multiple links, foreign owner, broad/null/unknown DACL,
/// excess security data or any failed Windows observation. It never changes ACLs.
pub fn admit_private_grant_file(file: File) -> io::Result<PrivateGrantFile> {
    let identity = windows::regular_file_identity(&file)?;
    let user = windows::effective_user_sid()?;
    let descriptor = windows::private_file_descriptor(&file)?;
    private_grant_policy::verify(&descriptor, &user)?;
    let admitted = PrivateGrantFile { file, identity, descriptor, user };
    admitted.revalidate()?;
    Ok(admitted)
}
