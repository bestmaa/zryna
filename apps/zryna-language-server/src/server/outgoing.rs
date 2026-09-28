use std::{collections::BTreeSet, io::Write, time::Instant};

use serde_json::{Value, json};
use zryna_driver::diagnostic_sessions::{DiagnosticRevision, QUERY_DEADLINE};

use super::{AnalysisProfile, RevisionCompiler, Server};
use crate::protocol::{self, Incoming};

/// Maximum request IDs retained until their terminal responses are emitted.
pub const MAX_OUTSTANDING_REQUESTS: usize = 64;

/// One response or notification awaiting successful transport emission.
///
/// Request completion state is intentionally opaque. The connection releases a request ID only
/// after [`super::Server::write_outgoing`] writes and flushes this value successfully.
#[derive(Debug)]
pub struct Outgoing {
    value: Value,
    completion: Option<String>,
    formatting_revision: Option<(DiagnosticRevision, Instant)>,
}

impl Outgoing {
    pub(super) fn untracked(value: Value) -> Self {
        Self { value, completion: None, formatting_revision: None }
    }

    pub(super) fn tracked(value: Value, request_id: String) -> Self {
        Self { value, completion: Some(request_id), formatting_revision: None }
    }

    pub(super) fn formatting(
        value: Value,
        request_id: String,
        revision: DiagnosticRevision,
        started: Instant,
    ) -> Self {
        Self { value, completion: Some(request_id), formatting_revision: Some((revision, started)) }
    }

    /// Returns the JSON-RPC value that will be framed on the connection.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }
}

#[derive(Debug)]
pub(super) struct OutstandingRequests {
    ids: BTreeSet<String>,
}

impl OutstandingRequests {
    pub(super) const fn new() -> Self {
        Self { ids: BTreeSet::new() }
    }

    pub(super) fn reserve(&mut self, request_id: String) -> bool {
        self.ids.len() < MAX_OUTSTANDING_REQUESTS && self.ids.insert(request_id)
    }
}

impl<Compiler: RevisionCompiler> Server<Compiler> {
    pub(super) fn handle_reserved(&mut self, message: Incoming) -> Vec<Outgoing> {
        let Some(request_id) = message.id.as_ref().map(|id| id.internal().to_owned()) else {
            return self.handle(message).into_iter().map(Outgoing::untracked).collect();
        };
        if !self.outstanding.reserve(request_id.clone()) {
            return vec![Outgoing::untracked(protocol::invalid_request())];
        }
        let mut values = self.handle(message);
        match values.len() {
            0 => Vec::new(),
            1 => values
                .pop()
                .map(|value| vec![Outgoing::tracked(value, request_id)])
                .unwrap_or_default(),
            _ => {
                vec![Outgoing::tracked(protocol::error(None, -32603, "Internal error"), request_id)]
            }
        }
    }

    /// Finishes all admitted definition requests after queued cancellation or edits were observed.
    #[must_use]
    pub fn finish_pending(&mut self) -> Vec<Outgoing> {
        let mut output = Vec::new();
        while let Some(pending) = self.pending.pop_front() {
            let response = self.session.finish_definition(pending.query, Instant::now());
            let request_id = pending.id.internal().to_owned();
            output.push(Outgoing::tracked(
                self.definition_response(
                    &pending.id,
                    pending.revision,
                    &pending.sources,
                    &response,
                ),
                request_id,
            ));
        }
        output.extend(self.finish_formatting());
        output
    }

    /// Writes one response or notification and releases its request reservation after flush.
    ///
    /// # Errors
    ///
    /// Returns an error without releasing a request reservation when serialization, framing, or
    /// flushing fails, or when an outgoing completion no longer belongs to this connection.
    pub fn write_outgoing(
        &mut self,
        output: &mut impl Write,
        mut outgoing: Outgoing,
    ) -> Result<(), String> {
        if outgoing
            .completion
            .as_ref()
            .is_some_and(|request_id| !self.outstanding.ids.contains(request_id))
        {
            return Err("language-server response completion is stale".to_owned());
        }
        if let Some((revision, started)) = outgoing.formatting_revision
            && outgoing.value.get("result").is_some()
        {
            let id = outgoing.value.get("id").cloned().and_then(protocol::decode_id);
            if started.elapsed() >= QUERY_DEADLINE {
                outgoing.value =
                    protocol::error(id.as_ref(), -32803, "Formatting deadline exceeded");
            } else {
                let current = self.session.active_revision() == Some(revision)
                    && (self.profile != AnalysisProfile::DataOwnership
                        || self.m3_graph_current(
                            revision,
                            QUERY_DEADLINE.saturating_sub(started.elapsed()),
                        ));
                if started.elapsed() >= QUERY_DEADLINE {
                    outgoing.value =
                        protocol::error(id.as_ref(), -32803, "Formatting deadline exceeded");
                } else if !current {
                    outgoing.value = protocol::error(id.as_ref(), -32801, "Content modified");
                    outgoing.value["error"]["data"] = json!({"code":"ZRYNA-D4002"});
                }
            }
        }
        let bytes = serde_json::to_vec(&outgoing.value).map_err(|error| error.to_string())?;
        crate::framing::write_frame(output, &bytes).map_err(|error| error.to_string())?;
        output.flush().map_err(|error| error.to_string())?;
        if let Some(request_id) = outgoing.completion
            && !self.outstanding.ids.remove(&request_id)
        {
            return Err("language-server response completion is stale".to_owned());
        }
        Ok(())
    }
}
