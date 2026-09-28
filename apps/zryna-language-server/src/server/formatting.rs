use std::{ops::Range, time::Instant};

use serde::Deserialize;
use serde_json::{Value, json};
use zryna_driver::diagnostic_sessions::{DiagnosticRevision, FormattingError, QUERY_DEADLINE};

use super::{AnalysisProfile, RevisionCompiler, Server};
use crate::{
    coordinates::{Position, byte_range_to_positions, position_to_byte},
    params::decode_params,
    protocol::{self, Incoming, RequestId},
};

pub(super) struct PendingFormatting {
    id: RequestId,
    revision: DiagnosticRevision,
    uri: String,
    range: Option<Range<u32>>,
    started: Instant,
    cancelled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FormattingParams {
    text_document: DocumentIdentifier,
    options: FormattingOptions,
    #[serde(default)]
    range: Option<Selection>,
    #[serde(default)]
    work_done_token: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentIdentifier {
    uri: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    start: Position,
    end: Position,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FormattingOptions {
    tab_size: u32,
    insert_spaces: bool,
}

impl<Compiler: RevisionCompiler> Server<Compiler> {
    pub(super) fn formatting(&mut self, message: Incoming) -> Vec<Value> {
        let Some(id) = message.id else { return Vec::new() };
        let Some(params) = decode_params::<FormattingParams>(message.params) else {
            return vec![protocol::invalid_params(Some(&id))];
        };
        let _ = (params.options.insert_spaces, params.work_done_token);
        if params.options.tab_size == 0
            || (message.method == "textDocument/rangeFormatting") != params.range.is_some()
        {
            return vec![protocol::invalid_params(Some(&id))];
        }
        let Some(document) = self.documents.get(&params.text_document.uri) else {
            return vec![protocol::invalid_params(Some(&id))];
        };
        let Some(active) = self.active.as_ref() else {
            return vec![format_error(&id, FormattingError::Unavailable)];
        };
        let range = if let Some(range) = params.range {
            let start = position_to_byte(&document.text, range.start, self.encoding);
            let end = position_to_byte(&document.text, range.end, self.encoding);
            match (start, end) {
                (Some(start), Some(end)) if start <= end => Some(start..end),
                _ => return vec![format_error(&id, FormattingError::Range)],
            }
        } else {
            None
        };
        if self.pending.len() + self.pending_formatting.len() >= 32 {
            return vec![format_error(&id, FormattingError::Limit)];
        }
        self.pending_formatting.push_back(PendingFormatting {
            id,
            revision: active.revision,
            uri: params.text_document.uri,
            range,
            started: Instant::now(),
            cancelled: false,
        });
        Vec::new()
    }

    pub(super) fn cancel_formatting(&mut self, id: &RequestId) {
        for pending in &mut self.pending_formatting {
            if pending.id.internal() == id.internal() {
                pending.cancelled = true;
            }
        }
    }

    pub(super) fn finish_formatting(&mut self) -> Vec<super::Outgoing> {
        let mut output = Vec::new();
        while let Some(pending) = self.pending_formatting.pop_front() {
            let value = self.formatting_response(&pending);
            output.push(super::Outgoing::formatting(
                value,
                pending.id.internal().to_owned(),
                pending.revision,
                pending.started,
            ));
        }
        output
    }

    fn formatting_response(&self, pending: &PendingFormatting) -> Value {
        if pending.cancelled {
            return protocol::error(Some(&pending.id), -32800, "Request cancelled");
        }
        if pending.started.elapsed() >= QUERY_DEADLINE {
            return protocol::error(Some(&pending.id), -32803, "Formatting deadline exceeded");
        }
        let Some(document) = self.documents.get(&pending.uri) else {
            return format_error(&pending.id, FormattingError::Stale);
        };
        let edits = match self.session.format_source(
            pending.revision,
            &document.path,
            pending.range.clone(),
        ) {
            Ok(edits) => edits,
            Err(error) => return format_error(&pending.id, error),
        };
        if self.profile == AnalysisProfile::DataOwnership {
            let budget = QUERY_DEADLINE.saturating_sub(pending.started.elapsed());
            let current = self.m3_graph_current(pending.revision, budget);
            if pending.started.elapsed() >= QUERY_DEADLINE {
                return protocol::error(Some(&pending.id), -32803, "Formatting deadline exceeded");
            }
            if !current {
                return format_error(&pending.id, FormattingError::Stale);
            }
        }
        let mut output = Vec::new();
        for edit in edits {
            let Some(range) =
                byte_range_to_positions(&document.text, edit.start, edit.end, self.encoding)
            else {
                return format_error(&pending.id, FormattingError::Range);
            };
            output.push(json!({"range":range,"newText":edit.text}));
        }
        let response = protocol::response(&pending.id, &json!(output));
        if serde_json::to_vec(&response).map_or(true, |bytes| bytes.len() > 1_048_576) {
            return format_error(&pending.id, FormattingError::Limit);
        }
        response
    }

    pub(super) fn m3_graph_current(
        &self,
        revision: DiagnosticRevision,
        budget: std::time::Duration,
    ) -> bool {
        if budget.is_zero() {
            return false;
        }
        let Some(root) = self.source_root.as_ref() else { return false };
        let Some(entry) =
            self.entry_uri.as_ref().and_then(|uri| self.documents.get(uri)).and_then(|document| {
                zryna_source::NormalizedSourcePath::new(document.path.clone()).ok()
            })
        else {
            return false;
        };
        let (Ok(overlays), Some(admitted)) =
            (crate::documents::source_map(&self.documents), self.session.active_sources(revision))
        else {
            return false;
        };
        self.compiler.revalidate_data_ownership_workspace(root, &overlays, &entry, admitted, budget)
    }
}

fn format_error(id: &RequestId, error: FormattingError) -> Value {
    let code = if error == FormattingError::Stale { -32801 } else { -32803 };
    let mut result = protocol::error(Some(id), code, "Formatting rejected; see diagnostic code");
    result["error"]["data"] = json!({"code":error.code()});
    result
}

#[cfg(test)]
mod tests {
    use super::{FormattingError, format_error};

    #[test]
    fn formatter_limit_has_stable_protocol_code_and_no_edits() {
        let request = crate::protocol::decode(
            br#"{"jsonrpc":"2.0","id":7,"method":"textDocument/formatting"}"#,
        )
        .expect("request");
        let response = format_error(&request.id.expect("id"), FormattingError::Limit);
        assert_eq!(response["error"]["data"]["code"], "ZRYNA-D4004");
        assert!(response.get("result").is_none());
    }
}
