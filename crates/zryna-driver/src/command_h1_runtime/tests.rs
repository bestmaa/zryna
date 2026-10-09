mod consume;
mod deadline;
mod denied_probes;
mod execution;
mod host;
mod preparation;
pub(in crate::command_h1_runtime) mod private_input;

use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

use zryna_frontend::{VerifiedFrontendProviderV4, WorkerError, syntax_v4};
use zryna_source::{SourceFileInput, SourceMap};

use super::*;
use private_input::PrivateInput;

struct FixtureProvider {
    bytes: Vec<u8>,
    calls: AtomicUsize,
}

impl VerifiedFrontendProviderV4 for FixtureProvider {
    fn analyze_verified_v4(
        &self,
        sources: &SourceMap,
    ) -> Result<syntax_v4::ProjectSyntaxSnapshot, WorkerError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let raw = syntax_v4::decode_snapshot(&self.bytes).expect("fixture protocol-v4 decode");
        Ok(syntax_v4::verify_snapshot(raw, sources).expect("actual source-map verification"))
    }
}

pub(in crate::command_h1_runtime) struct SourceFixture {
    pub(in crate::command_h1_runtime) sources: SourceMap,
    provider: FixtureProvider,
}

impl SourceFixture {
    pub(in crate::command_h1_runtime) fn new(name: &str) -> Self {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/wasi-command-source-fixtures");
        let text = std::fs::read_to_string(root.join(format!("{name}.zry")))
            .expect("actual command source fixture");
        let bytes = std::fs::read(root.join(format!("{name}.json")))
            .expect("actual provider syntax fixture");
        Self::from_parts(text, bytes)
    }

    fn from_parts(text: String, bytes: Vec<u8>) -> Self {
        let sources = SourceMap::build(vec![SourceFileInput { path: "src/main.zry".into(), text }])
            .expect("actual source authority");
        Self { sources, provider: FixtureProvider { bytes, calls: AtomicUsize::new(0) } }
    }

    pub(in crate::command_h1_runtime) fn prepare(
        &self,
        input: Option<&PrivateInput>,
        policy: &CommandH1HostPolicy,
    ) -> Result<PreparedCommandH1, crate::SourceToIrError> {
        prepare_command_h1(
            &self.provider,
            &self.sources,
            &zryna_backend_webassembly::pinned_wit_sources(),
            input.map(PrivateInput::path),
            policy,
        )
    }
}

#[track_caller]
fn rejected_preparation(
    source: &SourceFixture,
    input: Option<&PrivateInput>,
    policy: &CommandH1HostPolicy,
    code: &str,
) {
    let result = source.prepare(input, policy).err().expect("preparation rejects before engine");
    let crate::SourceToIrError::Rejected(diagnostics) = result else {
        panic!("unexpected fixture provider failure")
    };
    assert_eq!(diagnostics[0].code(), code);
}

#[track_caller]
fn returned(run: &ExecutedCommandH1, expected: CommandH1RunReturn) {
    assert_eq!(run.record().outcome(), &CommandH1Outcome::RunReturned { result: expected });
    assert_eq!(run.record().teardown(), CommandH1Teardown::Confirmed);
    assert_eq!(run.record().succeeded(), expected == CommandH1RunReturn::Ok);
}

fn fresh_recovery() {
    let source = SourceFixture::new("pure-entry");
    let policy = CommandH1HostPolicy::deny_all();
    let run = source
        .prepare(None, &policy)
        .expect("fresh preparation")
        .execute(&policy)
        .expect("fresh consuming execution");
    returned(&run, CommandH1RunReturn::Ok);
}
