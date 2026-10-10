mod admission;
mod epoch;
mod execution;
mod probe_execution;
pub(super) mod probes;
mod retirement;
mod startup;

use std::sync::{Arc, atomic::Ordering};

use zryna_backend_webassembly::ServerOperation;
use zryna_frontend::{
    VerifiedFrontendProvider, WorkerError, native_lexer::lex,
    native_parser::parse_v2_recovering_candidate, syntax_v2,
};
use zryna_source::{SourceFileInput, SourceMap};

use super::{Approval, Envelope, Error, Observation, Prepared, Server, prepared::Preparation};

const EMPTY: &[u8] = br#"{"world":"zryna:capability-profiles/server@0.1.0","requests":[]}"#;
const CLOCK: &[u8] = br#"{"world":"zryna:capability-profiles/server@0.1.0","requests":["clock"],"clock":{"monotonic_reads":1,"subscriptions":0,"timers":0}}"#;

struct Candidate;
impl VerifiedFrontendProvider for Candidate {
    fn analyze_verified(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v2::ProjectSyntaxSnapshot, WorkerError> {
        // A private test provider only: its candidate still passes the mandatory source verifier.
        let lexed = lex(sources).expect("test source lexes");
        let candidate = parse_v2_recovering_candidate(sources, &lexed).expect("test syntax");
        Ok(syntax_v2::verify_snapshot(candidate, sources).expect("mandatory syntax verification"))
    }
}

fn sources(status: &str) -> SourceMap {
    SourceMap::build(vec![SourceFileInput {
        path: "status.zry".into(),
        text: format!("export function status(): i32 {{ return {status}; }}"),
    }])
    .expect("bounded source")
}

fn envelope() -> Envelope {
    Envelope {
        requests: crate::limits(),
        memory_bytes: 65_536,
        fuel: 100_000,
        resources: 4,
        callbacks: 8,
    }
}

fn prepare(
    status: &str,
    operation: ServerOperation,
    document: &[u8],
    approval: Approval,
    envelope: Envelope,
    observation: &Arc<Observation>,
) -> Result<Prepared, Error> {
    Prepared::new(
        &Candidate,
        sources(status),
        Preparation { export: "status", operation, document, approval, envelope },
        Arc::clone(observation),
    )
}

fn server(
    status: &str,
    operation: ServerOperation,
    document: &[u8],
    approval: Approval,
    envelope: Envelope,
    observation: &Arc<Observation>,
) -> Server {
    Server::start(
        prepare(status, operation, document, approval, envelope, observation)
            .expect("authenticated prepared server"),
    )
    .expect("start")
}

fn clean(observation: &Observation) {
    assert_eq!(observation.live(), (0, 0), "actual stores and owned resource objects");
    assert_eq!(observation.input_copies.load(Ordering::SeqCst), 0, "execution-owned input objects");
    assert_eq!(
        observation.input_copy_bytes.load(Ordering::SeqCst),
        0,
        "execution-owned input bytes"
    );
    assert_eq!(
        observation.stores_created.load(Ordering::SeqCst),
        observation.stores_destroyed.load(Ordering::SeqCst),
        "actual Store data destruction"
    );
    assert_eq!(
        observation.created.load(Ordering::SeqCst),
        observation.destroyed.load(Ordering::SeqCst),
        "every resource destroyed exactly once"
    );
}
