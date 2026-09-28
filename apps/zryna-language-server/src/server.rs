use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Display,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

mod definitions;
mod formatting;
mod outgoing;
mod profiles;
use formatting::PendingFormatting;
use profiles::AnalysisProfile;

use outgoing::OutstandingRequests;
pub use outgoing::{MAX_OUTSTANDING_REQUESTS, Outgoing};

use serde_json::{Value, json};
use zryna_driver::WorkspaceSourceRoot;
use zryna_driver::diagnostic_sessions::{
    DiagnosticRevision, DiagnosticSession, PendingDefinitionQuery, ToolingCompiler,
    ToolingCompilerError,
};
use zryna_source::{NormalizedSourcePath, SourceMap};

use crate::{
    coordinates::PositionEncoding,
    diagnostics::{decode_report, standard_diagnostics},
    documents::{Document, source_map},
    params::{CancelParams, DidChangeParams, DidCloseParams, DidOpenParams, decode_params},
    protocol::{
        self, Incoming, RequestId, empty_params, log_invalid, log_message, response_message,
    },
};

/// Compiler boundary used by the transport to admit one immutable source revision.
pub trait RevisionCompiler {
    /// Stable configuration or admission failure.
    type Error: Display;

    /// Analyzes and admits the exact source map into the driver-owned session.
    ///
    /// # Errors
    ///
    /// Returns the compiler boundary's stable configuration or admission failure.
    fn admit(
        &mut self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
    ) -> Result<DiagnosticRevision, Self::Error>;

    /// Analyzes the explicit public M2 profile for one normalized entrypoint.
    ///
    /// # Errors
    ///
    /// Returns the compiler boundary's stable configuration or admission failure.
    fn admit_control_flow(
        &mut self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
        entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, Self::Error>;

    /// Admits M3 with exact open buffers and authenticated saved imports below one root.
    ///
    /// # Errors
    /// Returns stable source, provider, semantic or session rejection.
    fn admit_data_ownership_workspace(
        &mut self,
        session: &mut DiagnosticSession,
        root: &WorkspaceSourceRoot,
        overlays: SourceMap,
        entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, Self::Error> {
        let _ = root;
        self.admit_control_flow(session, overlays, entrypoint)
    }

    /// Rechecks saved dependency authority immediately before formatting edits.
    fn revalidate_data_ownership_workspace(
        &self,
        _root: &WorkspaceSourceRoot,
        _overlays: &SourceMap,
        _entrypoint: &NormalizedSourcePath,
        _admitted: &SourceMap,
        _budget: Duration,
    ) -> bool {
        true
    }
}

impl RevisionCompiler for ToolingCompiler {
    type Error = ToolingCompilerError;

    fn admit(
        &mut self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
    ) -> Result<DiagnosticRevision, Self::Error> {
        ToolingCompiler::admit(self, session, sources)
    }

    fn admit_control_flow(
        &mut self,
        session: &mut DiagnosticSession,
        sources: SourceMap,
        entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, Self::Error> {
        ToolingCompiler::admit_control_flow(self, session, sources, entrypoint)
    }

    fn admit_data_ownership_workspace(
        &mut self,
        session: &mut DiagnosticSession,
        root: &WorkspaceSourceRoot,
        overlays: SourceMap,
        entrypoint: &NormalizedSourcePath,
    ) -> Result<DiagnosticRevision, Self::Error> {
        ToolingCompiler::admit_data_ownership_workspace(self, session, root, &overlays, entrypoint)
    }

    fn revalidate_data_ownership_workspace(
        &self,
        root: &WorkspaceSourceRoot,
        overlays: &SourceMap,
        entrypoint: &NormalizedSourcePath,
        admitted: &SourceMap,
        budget: Duration,
    ) -> bool {
        ToolingCompiler::revalidate_data_ownership_workspace(
            self, root, overlays, entrypoint, admitted, budget,
        )
    }
}

#[derive(Clone, Debug)]
struct ActiveSnapshot {
    revision: DiagnosticRevision,
    sources: SourceMap,
}

struct PendingDefinition {
    id: RequestId,
    query: PendingDefinitionQuery,
    revision: DiagnosticRevision,
    sources: SourceMap,
}

/// One bounded LSP connection and its immutable compiler revisions.
pub struct Server<Compiler> {
    compiler: Compiler,
    session: DiagnosticSession,
    root_uri: Option<String>,
    root_path: Option<PathBuf>,
    source_root: Option<WorkspaceSourceRoot>,
    entry_uri: Option<String>,
    encoding: PositionEncoding,
    profile: AnalysisProfile,
    documents: BTreeMap<String, Document>,
    active: Option<ActiveSnapshot>,
    pending: VecDeque<PendingDefinition>,
    pending_formatting: VecDeque<PendingFormatting>,
    outstanding: OutstandingRequests,
    initialized: bool,
    shutting_down: bool,
    exit: bool,
}

impl<Compiler: RevisionCompiler> Server<Compiler> {
    /// Creates an uninitialized connection.
    ///
    /// # Errors
    ///
    /// Returns an error only if process-local session identity is exhausted.
    pub fn new(
        compiler: Compiler,
    ) -> Result<Self, zryna_driver::diagnostic_sessions::DiagnosticSessionError> {
        Ok(Self {
            compiler,
            session: DiagnosticSession::try_new()?,
            root_uri: None,
            root_path: None,
            source_root: None,
            entry_uri: None,
            encoding: PositionEncoding::Utf16,
            profile: AnalysisProfile::Scalar,
            documents: BTreeMap::new(),
            active: None,
            pending: VecDeque::new(),
            pending_formatting: VecDeque::new(),
            outstanding: OutstandingRequests::new(),
            initialized: false,
            shutting_down: false,
            exit: false,
        })
    }

    /// Captures one trusted workspace root before protocol initialization.
    ///
    /// # Errors
    /// Rejects unsafe, unavailable, or late root configuration.
    pub fn configure_workspace_root(&mut self, root: &Path) -> Result<(), String> {
        if self.initialized || self.root_path.is_some() || !self.documents.is_empty() {
            return Err("workspace root must be configured before initialization".to_owned());
        }
        let source_root = WorkspaceSourceRoot::capture(root).map_err(|error| error.to_string())?;
        self.root_path = Some(root.to_path_buf());
        self.source_root = Some(source_root);
        Ok(())
    }

    /// Decodes and handles one complete JSON-RPC payload.
    #[must_use]
    pub fn handle_bytes(&mut self, bytes: &[u8]) -> Vec<Outgoing> {
        match protocol::decode(bytes) {
            Ok(message) => self.handle_reserved(message),
            Err(error) => vec![Outgoing::untracked(error)],
        }
    }

    /// Returns whether a valid `exit` notification ended this connection.
    #[must_use]
    pub const fn should_exit(&self) -> bool {
        self.exit
    }

    /// Returns whether definition work is waiting for queued cancellation or revision changes.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty() || !self.pending_formatting.is_empty()
    }

    fn handle(&mut self, message: Incoming) -> Vec<Value> {
        if self.shutting_down && message.method != "exit" && message.method != "$/cancelRequest" {
            return message.id.as_ref().map_or_else(Vec::new, |id| {
                vec![protocol::error(Some(id), -32600, "Server is shutting down")]
            });
        }
        match message.method.as_str() {
            "initialize" => self.initialize(message),
            "initialized"
                if message.id.is_none()
                    && self.initialized
                    && empty_params(message.params.as_ref()) =>
            {
                Vec::new()
            }
            "textDocument/didOpen" if message.id.is_none() && self.initialized => {
                self.did_open(message.params)
            }
            "textDocument/didChange" if message.id.is_none() && self.initialized => {
                self.did_change(message.params)
            }
            "textDocument/didClose" if message.id.is_none() && self.initialized => {
                self.did_close(message.params)
            }
            "textDocument/definition" if message.id.is_some() && self.initialized => {
                self.definition(message)
            }
            "textDocument/formatting" | "textDocument/rangeFormatting"
                if message.id.is_some() && self.initialized =>
            {
                self.formatting(message)
            }
            "$/cancelRequest" if message.id.is_none() && self.initialized => {
                self.cancel(message.params)
            }
            "shutdown" if message.id.is_some() && self.initialized => {
                let Some(id) = message.id.as_ref() else {
                    return Vec::new();
                };
                if !empty_params(message.params.as_ref()) {
                    return vec![protocol::invalid_params(Some(id))];
                }
                self.shutting_down = true;
                vec![protocol::response(id, &Value::Null)]
            }
            "exit"
                if message.id.is_none()
                    && self.shutting_down
                    && empty_params(message.params.as_ref()) =>
            {
                self.exit = true;
                Vec::new()
            }
            _ => message
                .id
                .as_ref()
                .map_or_else(Vec::new, |id| vec![protocol::method_not_found(Some(id))]),
        }
    }

    fn did_open(&mut self, params: Option<Value>) -> Vec<Value> {
        let Some(params) = decode_params::<DidOpenParams>(params) else {
            return vec![log_invalid()];
        };
        let item = params.text_document;
        let Some(path) = self.path_for_uri(&item.uri) else {
            return vec![log_invalid()];
        };
        if item.language_id != "zryna"
            || item.version < 0
            || (self.profile != AnalysisProfile::DataOwnership && !self.documents.is_empty())
            || self.documents.contains_key(&item.uri)
        {
            return vec![log_invalid()];
        }
        if self.entry_uri.is_none() {
            self.entry_uri = Some(item.uri.clone());
        }
        let mut candidate = self.documents.clone();
        candidate
            .insert(item.uri.clone(), Document { path, text: item.text, version: item.version });
        let output = self.admit_documents(candidate);
        if !self.documents.contains_key(&item.uri) && self.entry_uri.as_deref() == Some(&item.uri) {
            self.entry_uri = None;
        }
        output
    }

    fn did_change(&mut self, params: Option<Value>) -> Vec<Value> {
        let Some(params) = decode_params::<DidChangeParams>(params) else {
            return vec![log_invalid()];
        };
        let [change] = params.content_changes.as_slice() else {
            return vec![log_invalid()];
        };
        let mut candidate = self.documents.clone();
        let Some(document) = candidate.get_mut(&params.text_document.uri) else {
            return vec![log_invalid()];
        };
        if params.text_document.version <= document.version {
            return vec![log_invalid()];
        }
        document.text.clone_from(&change.text);
        document.version = params.text_document.version;
        self.admit_documents(candidate)
    }

    fn did_close(&mut self, params: Option<Value>) -> Vec<Value> {
        let Some(params) = decode_params::<DidCloseParams>(params) else {
            return vec![log_invalid()];
        };
        let Some(closed) = self.documents.remove(&params.text_document.uri) else {
            return vec![log_invalid()];
        };
        if self.entry_uri.as_deref() == Some(&params.text_document.uri) {
            self.entry_uri = self.documents.keys().next().cloned();
        }
        let mut output = vec![protocol::notification(
            "textDocument/publishDiagnostics",
            &json!({
                "uri":params.text_document.uri,"version":closed.version,"diagnostics":[]
            }),
        )];
        if self.documents.is_empty() {
            match DiagnosticSession::try_new() {
                Ok(session) => self.session = session,
                Err(error) => output.push(log_message(&error.to_string())),
            }
            self.active = None;
        } else {
            output.extend(self.rebuild());
        }
        output
    }

    fn cancel(&mut self, params: Option<Value>) -> Vec<Value> {
        let Some(params) = decode_params::<CancelParams>(params) else {
            return vec![log_invalid()];
        };
        let Some(id) = protocol::decode_id(params.id) else {
            return vec![log_invalid()];
        };
        self.cancel_formatting(&id);
        let _ = self.session.cancel(id.internal());
        Vec::new()
    }

    fn rebuild(&mut self) -> Vec<Value> {
        let sources = match source_map(&self.documents) {
            Ok(sources) => sources,
            Err(error) => {
                self.active = None;
                if let Ok(session) = DiagnosticSession::try_new() {
                    self.session = session;
                }
                return vec![log_message(&error.to_string())];
            }
        };
        self.admit_sources(sources)
    }

    fn admit_documents(&mut self, documents: BTreeMap<String, Document>) -> Vec<Value> {
        let sources = match source_map(&documents) {
            Ok(sources) => sources,
            Err(error) => return vec![log_message(&error.to_string())],
        };
        self.documents = documents;
        self.admit_sources(sources)
    }

    fn admit_sources(&mut self, sources: SourceMap) -> Vec<Value> {
        let admitted = match self.profile {
            AnalysisProfile::Scalar => self.compiler.admit(&mut self.session, sources.clone()),
            AnalysisProfile::ControlFlow => {
                let Some(path) = self.documents.values().next().map(|document| &document.path)
                else {
                    return vec![log_message("missing M2 entrypoint")];
                };
                let Ok(entrypoint) = NormalizedSourcePath::new(path.clone()) else {
                    return vec![log_message("invalid M2 entrypoint")];
                };
                self.compiler.admit_control_flow(&mut self.session, sources.clone(), &entrypoint)
            }
            AnalysisProfile::DataOwnership => {
                let Some(root) = self.source_root.as_ref() else {
                    return vec![log_message("missing M3 workspace root")];
                };
                let Some(path) = self
                    .entry_uri
                    .as_ref()
                    .and_then(|uri| self.documents.get(uri))
                    .map(|document| &document.path)
                else {
                    return vec![log_message("missing M3 entrypoint")];
                };
                let Ok(entrypoint) = NormalizedSourcePath::new(path.clone()) else {
                    return vec![log_message("invalid M3 entrypoint")];
                };
                self.compiler.admit_data_ownership_workspace(
                    &mut self.session,
                    root,
                    sources.clone(),
                    &entrypoint,
                )
            }
        };
        match admitted {
            Ok(revision) => {
                let retained = self.session.active_sources(revision).cloned().unwrap_or(sources);
                self.active = Some(ActiveSnapshot { revision, sources: retained });
                self.publish_diagnostics(revision)
            }
            Err(error) => {
                let fallback = self.session.admit_unready(sources.clone());
                self.active = fallback.ok().map(|revision| ActiveSnapshot { revision, sources });
                vec![log_message(&error.to_string())]
            }
        }
    }

    fn publish_diagnostics(&mut self, revision: DiagnosticRevision) -> Vec<Value> {
        let request = json!({
            "query_version":1,"request_id":format!("diagnostics:{}", revision.revision()),
            "snapshot":revision.handle().to_string(),"revision":revision.revision(),
            "method":"diagnostics","params":{},"limits":{"work":100_000,"results":10_000}
        });
        let Ok(bytes) = serde_json::to_vec(&request) else {
            return vec![log_message("diagnostic request encoding failed")];
        };
        let pending = match self.session.begin_diagnostics(&bytes, Instant::now()) {
            Ok(pending) => pending,
            Err(response) => return vec![log_message(response_message(&response))],
        };
        let response = self.session.finish_diagnostics(pending, Instant::now());
        let Some(encoded) =
            response.encoded().and_then(|value| serde_json::from_str::<Value>(value).ok())
        else {
            return vec![log_message(response_message(&response))];
        };
        let Some(report) = decode_report(&encoded) else {
            return vec![log_message("diagnostic response validation failed")];
        };
        let versions = self
            .documents
            .iter()
            .map(|(uri, document)| json!({"uri":uri,"version":document.version}))
            .collect::<Vec<_>>();
        let mut output = vec![protocol::notification(
            "zryna/publishDiagnostics",
            &json!({
                "snapshot":revision.handle().to_string(),"revision":revision.revision(),"documents":versions,"report":report
            }),
        )];
        let Some(standard) = standard_diagnostics(&report, &self.documents, self.encoding) else {
            output.push(log_message("diagnostic coordinate conversion failed"));
            return output;
        };
        output.extend(standard.into_iter().map(|(uri, version, diagnostics)| {
            protocol::notification(
                "textDocument/publishDiagnostics",
                &json!({
                    "uri":uri,"version":version,"diagnostics":diagnostics
                }),
            )
        }));
        output
    }
}
