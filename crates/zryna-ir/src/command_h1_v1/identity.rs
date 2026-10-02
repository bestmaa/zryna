use std::sync::atomic::{AtomicU64, Ordering};
use zryna_source::SourceMapIdentity;

static NEXT_PROGRAM: AtomicU64 = AtomicU64::new(1);

/// Distinct issuing identity for an exact sealed command, preserved by clones.
/// This process-local identity is not an artifact hash or host grant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramIdentity {
    source: SourceMapIdentity,
    serial: u64,
}

impl ProgramIdentity {
    pub(super) fn issue(source: SourceMapIdentity) -> Option<Self> {
        NEXT_PROGRAM
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |serial| serial.checked_add(1))
            .ok()
            .map(|serial| Self { source, serial })
    }

    /// Returns the issuing source-map identity.
    #[must_use]
    pub const fn source_map(self) -> SourceMapIdentity {
        self.source
    }
}

/// Closed memory partition authenticated by the command H1 runtime identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryPartition {
    private: (),
}

impl MemoryPartition {
    pub(super) const H1: Self = Self { private: () };

    /// Fixed memory extent: 256 Wasm pages.
    pub const MEMORY_BYTES: u32 = 16_777_216;
    /// First language-owned byte, following static data.
    pub const LANGUAGE_START: u32 = 65_536;
    /// Exclusive language end and first canonical byte.
    pub const CANONICAL_START: u32 = 15_728_640;
    /// Exclusive canonical arena end.
    pub const CANONICAL_END: u32 = Self::MEMORY_BYTES;
}
