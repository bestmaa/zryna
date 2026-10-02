//! Explicit root approval and monotonic revocation; neither input parsing nor WIT grants it.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use zryna_diagnostics::Diagnostic;

/// Current root approval for the closed command host policy.
/// Clones share revocation, and an equal key in a new object cannot replace its identity.
#[derive(Clone)]
pub struct CommandH1HostPolicy {
    approval: Arc<Approval>,
}

struct Approval {
    key: Option<String>,
    revoked: AtomicBool,
}

impl CommandH1HostPolicy {
    /// Creates an empty root grant for a pure command.
    #[must_use]
    pub fn deny_all() -> Self {
        Self::new(None)
    }

    /// Approves the exact one-key environment requirement under fixed quotas.
    ///
    /// # Errors
    /// Rejects an empty key or one exceeding 64 UTF-8 bytes.
    pub fn environment(key: &str) -> Result<Self, Diagnostic> {
        if key.is_empty() || key.len() > 64 {
            return Err(super::invalid());
        }
        Ok(Self::new(Some(key.to_owned())))
    }

    fn new(key: Option<String>) -> Self {
        Self { approval: Arc::new(Approval { key, revoked: AtomicBool::new(false) }) }
    }

    /// Permanently revokes this approval and every clone of it.
    pub fn revoke(&self) {
        self.approval.revoked.store(true, Ordering::Release);
    }

    pub(super) fn key(&self) -> Option<&str> {
        self.approval.key.as_deref()
    }
    pub(super) fn active(&self) -> bool {
        !self.approval.revoked.load(Ordering::Acquire)
    }
    pub(super) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.approval, &other.approval) && self.active() && other.active()
    }
}
