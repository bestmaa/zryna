//! Revision-bound compiler query sessions for bounded tooling transports.
//!
//! This compiler-host boundary retains exact source-map, structured-diagnostics-v2, and the first
//! semantics-owned definition authority. Transports may compose this boundary but cannot inspect
//! or reconstruct its semantic records. Verified scalar revisions also retain presentation-only
//! formatting plans. This module grants no edit application or execution authority.

use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    sync::{
        Arc, Weak,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use zryna_diagnostics::{Diagnostic, Severity, protocol_v2};
use zryna_frontend::{WorkerError, WorkerFailure};
use zryna_semantics::definition_queries::DefinitionIndex;
use zryna_source::{NormalizedSourcePath, SourceMap, SourceMapIdentity};

mod formatting;
mod request;
mod response;
mod retention;
mod scheduling;
mod semantic_definition;
#[cfg(feature = "diagnostic-test-support")]
mod test_support;
mod tooling_compiler;
mod tooling_execution;

use formatting::FormattingDocument;
pub use formatting::{FormattingEdit, FormattingError};
pub use response::{DiagnosticQueryResponse, QueryReason, QueryStatus};
use retention::{checked_cache_charge, source_fingerprint_and_charge};
pub use semantic_definition::PendingDefinitionQuery;
#[cfg(feature = "diagnostic-test-support")]
pub use test_support::admit_single_function_fixture;
pub use tooling_compiler::{ToolingCompiler, ToolingCompilerError};

/// Maximum complete encoded request bytes, including whitespace.
pub const MAX_REQUEST_BYTES: usize = 65_536;
/// Maximum complete encoded response bytes.
pub const MAX_RESPONSE_BYTES: usize = 1_048_576;
/// Maximum JSON object/array nesting in one request.
pub const MAX_REQUEST_DEPTH: u32 = 64;
/// Maximum logical work units admitted by one request.
pub const MAX_QUERY_WORK: u64 = 100_000;
/// Maximum result locations or edits admitted by one request.
pub const MAX_QUERY_RESULTS: u64 = 10_000;
/// Maximum retained immutable revisions in one session.
pub const MAX_RETAINED_REVISIONS: usize = 2;
/// Maximum deterministically charged cache bytes in one session.
pub const MAX_SESSION_CACHE_BYTES: usize = 64 * 1_024 * 1_024;
/// Maximum queued plus running requests in one session.
pub const MAX_IN_FLIGHT_REQUESTS: usize = 32;
/// Maximum request lifetime, including cancellation cleanup.
pub const QUERY_DEADLINE: Duration = Duration::from_secs(30);
/// Maximum request or snapshot identifier bytes.
pub const MAX_ID_BYTES: usize = 128;
/// Largest exact JSON integer admitted for a session revision.
pub const MAX_REVISION: u64 = (1_u64 << 53) - 1;

const LIVE: u8 = 0;
const CANCELLED: u8 = 1;
const TIMED_OUT: u8 = 2;
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

/// Opaque compiler-host-issued identity for one retained source revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticSnapshotHandle {
    session: u64,
    serial: u64,
}

impl fmt::Display for DiagnosticSnapshotHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "s{:016x}-{:013x}", self.session, self.serial)
    }
}

/// Immutable description of one admitted source revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticRevision {
    handle: DiagnosticSnapshotHandle,
    revision: u64,
    source_fingerprint: [u8; 32],
}

impl DiagnosticRevision {
    /// Returns the opaque session-bound handle.
    #[must_use]
    pub const fn handle(&self) -> DiagnosticSnapshotHandle {
        self.handle
    }

    /// Returns the monotonic nonzero revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the exact source-closure fingerprint specified by tooling snapshot v1.
    #[must_use]
    pub const fn source_fingerprint(&self) -> [u8; 32] {
        self.source_fingerprint
    }
}

/// Admission failures that cannot be represented as a correlated query response.
#[derive(Debug)]
pub enum DiagnosticSessionError {
    /// The process-local session identity space was exhausted.
    SessionIdentityExhausted,
    /// The exact JavaScript-safe revision space was exhausted.
    RevisionExhausted,
    /// The retained revision alone exceeds the 64 MiB cache charge.
    SessionCacheExhausted,
    /// Structured diagnostic v2 rejected producer input or source authority.
    Diagnostics(protocol_v2::ProtocolError),
    /// Semantic analysis rejected the source before query facts could be retained.
    Semantics(Vec<Diagnostic>),
    /// Semantic inputs were not issued from the retained source authority.
    SemanticAuthority,
    /// A previously verified source map could not be enumerated consistently.
    SourceInvariant,
}

impl fmt::Display for DiagnosticSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SessionIdentityExhausted => "diagnostic session identity space is exhausted",
            Self::RevisionExhausted => "diagnostic session revision space is exhausted",
            Self::SessionCacheExhausted => "diagnostic session cache limit exceeded",
            Self::Diagnostics(_) => "structured diagnostics rejected the retained source",
            Self::Semantics(_) => "semantic analysis rejected the retained source",
            Self::SemanticAuthority => "semantic inputs are not bound to the retained source",
            Self::SourceInvariant => "retained source map could not be enumerated",
        })
    }
}

impl std::error::Error for DiagnosticSessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Diagnostics(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct RevisionRecord {
    description: DiagnosticRevision,
    sources: SourceMap,
    report: Option<Arc<str>>,
    definitions: Option<Arc<DefinitionIndex>>,
    cache_bytes: usize,
    formatting: Vec<FormattingDocument>,
}

#[derive(Clone, Debug)]
struct Correlation {
    query_version: u64,
    request_id: String,
    snapshot: String,
    revision: u64,
}

/// One admitted diagnostics request awaiting bounded completion.
#[derive(Debug)]
pub struct PendingDiagnosticQuery {
    session: u64,
    correlation: Correlation,
    revision: DiagnosticRevision,
    record: Weak<RevisionRecord>,
    source_identity: SourceMapIdentity,
    work_limit: u64,
    deadline: Instant,
    state: Arc<AtomicU8>,
}

#[derive(Debug)]
struct RequestSlot {
    state: Arc<AtomicU8>,
    deadline: Instant,
}

/// One compiler-host-owned collection of immutable diagnostic revisions.
#[derive(Debug)]
pub struct DiagnosticSession {
    session: u64,
    next_revision: u64,
    retained: VecDeque<Arc<RevisionRecord>>,
    cache_bytes: usize,
    in_flight: BTreeMap<String, RequestSlot>,
}

impl DiagnosticSession {
    /// Creates a fresh session whose handles cannot be reused by another live session.
    ///
    /// # Errors
    ///
    /// Returns an error if the process-local identity counter is exhausted.
    pub fn try_new() -> Result<Self, DiagnosticSessionError> {
        let session = NEXT_SESSION_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| current.checked_add(1))
            .map_err(|_| DiagnosticSessionError::SessionIdentityExhausted)?;
        Ok(Self {
            session,
            next_revision: 1,
            retained: VecDeque::new(),
            cache_bytes: 0,
            in_flight: BTreeMap::new(),
        })
    }

    /// Admits an immutable source revision with a complete v2 diagnostic pass.
    ///
    /// # Errors
    ///
    /// Rejects foreign spans, invalid diagnostic records, revision exhaustion, source invariant
    /// failure, or a first revision whose deterministic cache charge exceeds 64 MiB.
    pub fn admit_diagnostics(
        &mut self,
        sources: SourceMap,
        diagnostics: &[Diagnostic],
    ) -> Result<DiagnosticRevision, DiagnosticSessionError> {
        let report = protocol_v2::render_json(diagnostics, &sources)
            .map_err(DiagnosticSessionError::Diagnostics)?;
        self.admit(sources, Some(report.into()), None, Vec::new())
    }

    pub(crate) fn admit_worker_failure(
        &mut self,
        sources: SourceMap,
        error: &WorkerError,
    ) -> Result<DiagnosticRevision, DiagnosticSessionError> {
        if !error.diagnostics().is_empty() {
            return self.admit_diagnostics(sources, error.diagnostics());
        }
        let guidance = if error.failure() == WorkerFailure::ProviderRejected {
            "remove unsupported syntax or top-level declarations; check the selected profile"
        } else {
            "verify the pinned Node.js runtime and TypeScript frontend, then retry"
        };
        self.admit_diagnostics(
            sources,
            &[Diagnostic::error(error.code(), None, error.to_string(), guidance)],
        )
    }

    /// Admits successful protocol-v2 semantic state for definition queries.
    ///
    /// # Errors
    ///
    /// Rejects foreign authority, semantic diagnostics, invalid reports, or retention exhaustion.
    pub fn admit_semantics(
        &mut self,
        sources: SourceMap,
        syntax: &zryna_frontend::syntax_v2::ProjectSyntaxSnapshot,
    ) -> Result<DiagnosticRevision, DiagnosticSessionError> {
        let input = zryna_semantics::SemanticInput::try_new(syntax, &sources)
            .ok_or(DiagnosticSessionError::SemanticAuthority)?;
        let definitions =
            DefinitionIndex::analyze(input).map_err(DiagnosticSessionError::Semantics)?;
        if !definitions.is_bound_to(&sources) {
            return Err(DiagnosticSessionError::SemanticAuthority);
        }
        let report = protocol_v2::render_json(syntax.diagnostics(), &sources)
            .map_err(DiagnosticSessionError::Diagnostics)?;
        {
            let formatting = FormattingDocument::prepare(syntax, &sources);
            self.admit(
                sources,
                Some(report.into()),
                Some(Arc::new(definitions)),
                formatting.into_iter().collect(),
            )
        }
    }

    /// Admits one verified protocol-v2 analysis for tooling diagnostics and definition queries.
    ///
    /// Provider errors retain their exact diagnostic report without semantic facts. Successfully
    /// parsed sources retain definition facts only when the existing scalar semantic checker
    /// succeeds; semantic rejection is retained as diagnostics instead of fabricated query data.
    ///
    /// # Errors
    ///
    /// Rejects foreign source authority, invalid diagnostics, or retention exhaustion.
    pub fn admit_analysis(
        &mut self,
        sources: SourceMap,
        syntax: &zryna_frontend::syntax_v2::ProjectSyntaxSnapshot,
    ) -> Result<DiagnosticRevision, DiagnosticSessionError> {
        if !syntax.is_bound_to(&sources) {
            return Err(DiagnosticSessionError::SemanticAuthority);
        }
        let Some(input) = zryna_semantics::SemanticInput::try_new(syntax, &sources) else {
            return self.admit_diagnostics(sources, syntax.diagnostics());
        };
        match DefinitionIndex::analyze(input) {
            Ok(definitions) if definitions.is_bound_to(&sources) => {
                let report = protocol_v2::render_json(syntax.diagnostics(), &sources)
                    .map_err(DiagnosticSessionError::Diagnostics)?;
                {
                    let formatting = FormattingDocument::prepare(syntax, &sources);
                    self.admit(
                        sources,
                        Some(report.into()),
                        Some(Arc::new(definitions)),
                        formatting.into_iter().collect(),
                    )
                }
            }
            Ok(_) => Err(DiagnosticSessionError::SemanticAuthority),
            Err(diagnostics) => self.admit_diagnostics(sources, &diagnostics),
        }
    }

    /// Admits one exact in-memory protocol-v3 M2 analysis without loading source files.
    ///
    /// The selected entry belongs to this source map. Verified M2 semantics owns names, types,
    /// direct calls, and control flow; scalar definition facts are not inferred from v3 syntax.
    /// Provider and semantic errors are retained as ready structured diagnostics.
    ///
    /// # Errors
    ///
    /// Rejects foreign source authority, an absent entry, invalid diagnostics, or retention
    /// exhaustion.
    pub fn admit_control_flow_analysis(
        &mut self,
        sources: SourceMap,
        syntax: &zryna_frontend::syntax_v3::ProjectSyntaxSnapshot,
        entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, DiagnosticSessionError> {
        if !syntax.is_bound_to(&sources) {
            return Err(DiagnosticSessionError::SemanticAuthority);
        }
        let entry = sources.file_id(entrypoint).ok_or(DiagnosticSessionError::SemanticAuthority)?;
        if syntax.diagnostics().iter().any(|diagnostic| diagnostic.severity() == Severity::Error) {
            return self.admit_diagnostics(sources, syntax.diagnostics());
        }
        let input =
            zryna_semantics::control_flow_v1::SemanticInput::try_new(syntax, &sources, entry)
                .ok_or(DiagnosticSessionError::SemanticAuthority)?;
        match zryna_semantics::control_flow_v1::lower(input) {
            Ok(_) => {
                let report = protocol_v2::render_json(syntax.diagnostics(), &sources)
                    .map_err(DiagnosticSessionError::Diagnostics)?;
                let formatting = FormattingDocument::prepare_control_flow(syntax, &sources);
                self.admit(sources, Some(report.into()), None, formatting.into_iter().collect())
            }
            Err(diagnostics) => self.admit_diagnostics(sources, &diagnostics),
        }
    }

    /// Admits an exact protocol-v4 M3 source map after verified ownership semantics.
    ///
    /// # Errors
    /// Rejects foreign source authority, an absent entry, invalid diagnostics, or retention
    /// exhaustion. Failed semantic analysis retains diagnostics without formatting edits.
    pub fn admit_data_ownership_analysis(
        &mut self,
        sources: SourceMap,
        syntax: &zryna_frontend::syntax_v4::ProjectSyntaxSnapshot,
        entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, DiagnosticSessionError> {
        if !syntax.is_bound_to(&sources) {
            return Err(DiagnosticSessionError::SemanticAuthority);
        }
        let entry = sources.file_id(entrypoint).ok_or(DiagnosticSessionError::SemanticAuthority)?;
        if syntax.diagnostics().iter().any(|diagnostic| diagnostic.severity() == Severity::Error) {
            return self.admit_diagnostics(sources, syntax.diagnostics());
        }
        let input =
            zryna_semantics::data_ownership_v1::SemanticInput::try_new(syntax, &sources, entry)
                .ok_or(DiagnosticSessionError::SemanticAuthority)?;
        match zryna_semantics::data_ownership_v1::lower(input) {
            Ok(_) => {
                let report = protocol_v2::render_json(syntax.diagnostics(), &sources)
                    .map_err(DiagnosticSessionError::Diagnostics)?;
                let formatting = FormattingDocument::prepare_data_ownership(syntax, &sources)
                    .ok_or(DiagnosticSessionError::SemanticAuthority)?;
                self.admit(sources, Some(report.into()), None, formatting)
            }
            Err(diagnostics) => self.admit_diagnostics(sources, &diagnostics),
        }
    }

    /// Admits source authority whose diagnostic pass is not ready yet.
    ///
    /// # Errors
    ///
    /// Rejects revision exhaustion, source invariant failure, or a cache charge over 64 MiB.
    pub fn admit_unready(
        &mut self,
        sources: SourceMap,
    ) -> Result<DiagnosticRevision, DiagnosticSessionError> {
        self.admit(sources, None, None, Vec::new())
    }

    /// Returns the currently active revision, when one has been admitted.
    #[must_use]
    pub fn active_revision(&self) -> Option<DiagnosticRevision> {
        self.retained.back().map(|record| record.description)
    }

    /// Returns the exact immutable source map retained for the active revision.
    #[must_use]
    pub fn active_sources(&self, revision: DiagnosticRevision) -> Option<&SourceMap> {
        self.retained
            .back()
            .filter(|record| record.description == revision)
            .map(|record| &record.sources)
    }

    /// Returns the number of immutable revisions retained for bounded incremental reuse.
    #[must_use]
    pub fn retained_revisions(&self) -> usize {
        self.retained.len()
    }

    /// Returns the exact variable-byte cache charge for retained paths, source, and v2 reports.
    #[must_use]
    pub const fn cache_bytes(&self) -> usize {
        self.cache_bytes
    }

    fn admit(
        &mut self,
        sources: SourceMap,
        report: Option<Arc<str>>,
        definitions: Option<Arc<DefinitionIndex>>,
        formatting: Vec<FormattingDocument>,
    ) -> Result<DiagnosticRevision, DiagnosticSessionError> {
        if self.next_revision > MAX_REVISION {
            return Err(DiagnosticSessionError::RevisionExhausted);
        }
        let (source_fingerprint, source_bytes) = source_fingerprint_and_charge(&sources)?;
        let semantic_bytes = match definitions.as_deref() {
            Some(definitions) => {
                definitions.cache_bytes().ok_or(DiagnosticSessionError::SessionCacheExhausted)?
            }
            None => 0,
        };
        let cache_bytes = checked_cache_charge(
            source_bytes,
            report.as_ref().map_or(0, |value| value.len()),
            semantic_bytes + formatting.iter().map(FormattingDocument::cache_bytes).sum::<usize>(),
        )?;
        let description = DiagnosticRevision {
            handle: DiagnosticSnapshotHandle { session: self.session, serial: self.next_revision },
            revision: self.next_revision,
            source_fingerprint,
        };
        let record = Arc::new(RevisionRecord {
            description,
            sources,
            report,
            definitions,
            cache_bytes,
            formatting,
        });
        let mut projected_cache = self
            .cache_bytes
            .checked_add(cache_bytes)
            .ok_or(DiagnosticSessionError::SessionCacheExhausted)?;
        let mut evictions = 0_usize;
        while self.retained.len() + 1 - evictions > MAX_RETAINED_REVISIONS
            || projected_cache > MAX_SESSION_CACHE_BYTES
        {
            let evicted =
                self.retained.get(evictions).ok_or(DiagnosticSessionError::SourceInvariant)?;
            projected_cache = projected_cache
                .checked_sub(evicted.cache_bytes)
                .ok_or(DiagnosticSessionError::SourceInvariant)?;
            evictions += 1;
        }
        self.mark_replaced();
        self.next_revision += 1;
        for _ in 0..evictions {
            let _ = self.retained.pop_front();
        }
        self.retained.push_back(record);
        self.cache_bytes = projected_cache;
        Ok(description)
    }

    fn mark_replaced(&mut self) {
        self.in_flight.clear();
    }
}

#[cfg(test)]
mod tests;
