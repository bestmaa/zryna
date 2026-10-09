//! Conditional reverse cleanup of still-live obligations, including untaken outputs.

use super::Frame;

/// Exact conditional release obligation. Constructing a copy supplies no body authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupEntry {
    pub(super) owner: usize,
    pub(super) call: usize,
    pub(super) primary_slot: usize,
    pub(super) releasable_on_malformed: bool,
    pub(super) validation_required: bool,
}
impl CleanupEntry {
    /// Stable obligation identity within the creating function.
    #[must_use]
    pub const fn owner_id(&self) -> usize {
        self.owner
    }
    /// Exact creating call, whose successful non-null result is the live condition.
    #[must_use]
    pub const fn creating_call(&self) -> usize {
        self.call
    }
    /// Caller slot whose non-null output determines whether an allocation exists.
    #[must_use]
    pub const fn primary_output_slot(&self) -> usize {
        self.primary_slot
    }
    /// Untaken outputs still require their declaration's metadata validation before release.
    #[must_use]
    pub const fn validation_required(&self) -> bool {
        self.validation_required
    }
    /// An invalid output may be released only under this exact captured library promise.
    #[must_use]
    pub const fn releasable_on_malformed(&self) -> bool {
        self.releasable_on_malformed
    }
}

impl Frame<'_> {
    pub(super) fn cleanup(&self, nonzero_call: Option<usize>) -> Vec<CleanupEntry> {
        self.owners
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(owner, entry)| {
                if !entry.live || nonzero_call == Some(entry.call) {
                    return None;
                }
                Some(CleanupEntry {
                    owner,
                    call: entry.call,
                    primary_slot: entry.primary_slot,
                    releasable_on_malformed: entry.releasable_on_malformed,
                    validation_required: !entry.validated,
                })
            })
            .collect()
    }
}
