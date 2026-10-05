//! Isolated v5 transport sharing only the established process/framing engine.
mod model;
mod wire;
use super::{
    ANALYZE_ID, AnalyzeWireRequest, ChildGuard, Duration, HANDSHAKE_ID, HandshakeRequest, Instant,
    MAX_HANDSHAKE_RESPONSE_BYTES, MAX_RESPONSE_LINES, MIN_WORKER_TIMEOUT, SourceMap, StreamState,
    WorkerError, WorkerFailure, build_analyze_request, cleanup_reserve, finalize_process,
    finish_process, mpsc, parse_response, receive_response_line, serialize_request,
    spawn_stderr_reader, spawn_stdout_reader, spawn_worker, syntax_v4,
};
pub use model::{
    FrontendCapabilitiesV5, ProviderExpectationV5, ProviderInfoV5, WorkerLimitsV5, WorkerSpecV5,
};
use zryna_syntax::v5 as syntax_v5;

/// Maximum stdout for the v5 envelope; v5 retains v4's response byte cap.
pub const MAX_WORKER_STDOUT_BYTES_V5: usize =
    MAX_HANDSHAKE_RESPONSE_BYTES + syntax_v4::MAX_RESPONSE_BYTES + MAX_RESPONSE_LINES;

/// A configured protocol-v5 worker that returns source-map-verified syntax only.
#[derive(Clone, Debug)]
pub struct WorkerFrontendV5 {
    spec: WorkerSpecV5,
}

/// Provider abstraction whose only output is source-map-verified protocol-v5 syntax.
pub trait VerifiedFrontendProviderV5: Send + Sync {
    /// Returns the minimum caller-supplied timeout required by this provider.
    fn minimum_analysis_timeout(&self) -> Duration {
        Duration::ZERO
    }

    /// Authenticates the exact v5 provider and analyzes the authoritative source map.
    ///
    /// # Errors
    ///
    /// Returns a fail-closed worker error before untrusted syntax reaches a compiler consumer.
    fn analyze_verified_v5(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v5::VerifiedProjectSyntaxV5, WorkerError>;

    /// Analyzes with a caller-tightened whole-session timeout.
    ///
    /// # Errors
    ///
    /// Returns the provider's fail-closed analysis error.
    fn analyze_verified_v5_with_timeout(
        &self,
        sources: &SourceMap,
        _timeout: Duration,
    ) -> Result<syntax_v5::VerifiedProjectSyntaxV5, WorkerError> {
        self.analyze_verified_v5(sources)
    }
}

impl WorkerFrontendV5 {
    /// Creates a protocol-v5 worker frontend from a validated command specification.
    #[must_use]
    pub const fn new(spec: WorkerSpecV5) -> Self {
        Self { spec }
    }

    /// Returns the protocol-v5 worker command specification.
    #[must_use]
    pub const fn spec(&self) -> &WorkerSpecV5 {
        &self.spec
    }

    /// Authenticates one fresh exact-v5 worker, analyzes the source map, and verifies its reply.
    ///
    /// The schema-five analysis request is not written until provider identity, runtime version,
    /// protocol version, and all five capabilities match the trusted expectation exactly.
    ///
    /// # Errors
    ///
    /// Returns a stable fail-closed worker failure for configuration, process, framing, identity,
    /// budget, provider, decoding, or protocol-v5 syntax-verification errors.
    pub fn analyze_verified_v5(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v5::VerifiedProjectSyntaxV5, WorkerError> {
        let mut analyze_request = build_analyze_request(sources)?;
        analyze_request.schema_version = syntax_v5::PROTOCOL_VERSION;
        let handshake_bytes =
            serialize_request(&HandshakeRequest { id: HANDSHAKE_ID, method: "handshake" })?;
        let analyze_bytes = serialize_request(&AnalyzeWireRequest {
            id: ANALYZE_ID,
            method: "analyze",
            params: &analyze_request,
        })?;

        let spawned = spawn_worker(&self.spec)?;
        let deadline = Instant::now()
            .checked_add(self.spec.limits.timeout)
            .ok_or_else(|| WorkerError::new(WorkerFailure::Configuration))?;
        let operation_deadline = deadline
            .checked_sub(cleanup_reserve(self.spec.limits.timeout))
            .ok_or_else(|| WorkerError::new(WorkerFailure::Configuration))?;
        let mut process = ChildGuard::new(spawned.process, spawned.stdin);
        let (sender, receiver) = mpsc::sync_channel(8);
        process.tasks.push(spawn_stdout_reader(
            spawned.stdout,
            sender.clone(),
            self.spec.limits.stdout_bytes,
        ));
        process.tasks.push(spawn_stderr_reader(
            spawned.stderr,
            sender.clone(),
            self.spec.limits.stderr_bytes,
        ));
        let mut stream_state = StreamState::default();

        let operation = (|| {
            process.write_request(&handshake_bytes)?;
            let handshake_line = receive_response_line(
                &receiver,
                &mut stream_state,
                operation_deadline,
                MAX_HANDSHAKE_RESPONSE_BYTES,
            )?;
            let handshake: ProviderInfoV5 = parse_response(&handshake_line, HANDSHAKE_ID)?;
            verify_handshake(&handshake, &self.spec.expected)?;

            process.write_request_async(analyze_bytes, sender.clone())?;
            let snapshot_line = receive_response_line(
                &receiver,
                &mut stream_state,
                operation_deadline,
                syntax_v4::MAX_RESPONSE_BYTES,
            )?;
            let decoded: wire::ClosedValue = parse_response(&snapshot_line, ANALYZE_ID)?;
            let canonical = serde_json::to_vec(&decoded.0)
                .map_err(|_| WorkerError::new(WorkerFailure::InvalidResponse))?;
            let raw = syntax_v5::decode_snapshot(&canonical)
                .map_err(|_| WorkerError::new(WorkerFailure::InvalidResponse))?;
            let status =
                finish_process(&mut process, &receiver, &mut stream_state, operation_deadline)?;
            if !status.success() {
                return Err(WorkerError::new(WorkerFailure::ProcessExit));
            }
            syntax_v5::verify_snapshot(raw, sources).map_err(source_verification_error)
        })();

        finalize_process(&mut process, &receiver, &mut stream_state, sender, deadline, operation)
    }

    /// Runs one exact-v5 analysis with a timeout no greater than this frontend's configured cap.
    ///
    /// # Errors
    ///
    /// Returns a configuration error when the requested timeout is outside the hard worker range,
    /// or the same fail-closed errors as [`Self::analyze_verified_v5`].
    pub fn analyze_verified_v5_with_timeout(
        &self,
        sources: &SourceMap,
        timeout: Duration,
    ) -> Result<syntax_v5::VerifiedProjectSyntaxV5, WorkerError> {
        let timeout = timeout.min(self.spec.limits.timeout());
        let limits = WorkerLimitsV5::new(
            timeout,
            self.spec.limits.stdout_bytes(),
            self.spec.limits.stderr_bytes(),
        )?;
        let mut spec = self.spec.clone();
        spec.limits = limits;
        Self::new(spec).analyze_verified_v5(sources)
    }
}

impl VerifiedFrontendProviderV5 for WorkerFrontendV5 {
    fn minimum_analysis_timeout(&self) -> Duration {
        MIN_WORKER_TIMEOUT
    }

    fn analyze_verified_v5(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v5::VerifiedProjectSyntaxV5, WorkerError> {
        Self::analyze_verified_v5(self, sources)
    }

    fn analyze_verified_v5_with_timeout(
        &self,
        sources: &SourceMap,
        timeout: Duration,
    ) -> Result<syntax_v5::VerifiedProjectSyntaxV5, WorkerError> {
        Self::analyze_verified_v5_with_timeout(self, sources, timeout)
    }
}

/// Stable frontend-worker failure category.
fn verify_handshake(
    actual: &ProviderInfoV5,
    expected: &ProviderExpectationV5,
) -> Result<(), WorkerError> {
    if actual.provider != expected.provider {
        return Err(WorkerError::new(WorkerFailure::ProviderIdentity));
    }
    if actual.provider_version != expected.provider_version {
        return Err(WorkerError::new(WorkerFailure::ProviderVersion));
    }
    if actual.protocol_version != syntax_v5::PROTOCOL_VERSION {
        return Err(WorkerError::new(WorkerFailure::ProviderProtocol));
    }
    if actual.capabilities != expected.capabilities {
        return Err(WorkerError::new(WorkerFailure::ProviderCapabilities));
    }
    Ok(())
}

fn source_verification_error(errors: Vec<syntax_v5::DeclarationError>) -> WorkerError {
    let diagnostics = errors
        .into_iter()
        .map(|error| {
            let message = "protocol-v5 source or declaration claim failed verification";
            let guidance = "use source-faithful bounded generic syntax";
            error.span.map_or_else(
                || zryna_diagnostics::Diagnostic::error(error.code, None, message, guidance),
                |span| zryna_diagnostics::Diagnostic::error_at(error.code, span, message, guidance),
            )
        })
        .collect();
    WorkerError::snapshot_verification(diagnostics)
}
