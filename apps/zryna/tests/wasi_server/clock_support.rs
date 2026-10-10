use super::super::support::{Case, Running, common};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, process::Command};

pub(super) struct Granted {
    pub case: Case,
    pub request: common::Input,
    pub approval: common::Input,
    pub bytes: Vec<u8>,
}
impl Granted {
    pub fn new(attempts: u16) -> Self {
        Self::for_case(Case::new(attempts), document())
    }
    pub fn for_case(case: Case, bytes: Vec<u8>) -> Self {
        let request = common::Input::from_bytes(&bytes);
        let approval = common::Input::from_bytes(
            &serde_json::to_vec(&json!({
                "schema":"zryna.wasi-server-guest-approval.v1",
                "request_sha256":format!("{:x}",Sha256::digest(&bytes)),"monotonic_reads":1
            }))
            .expect("separate guest approval"),
        );
        Self { case, request, approval, bytes }
    }
    pub fn command(&self, source: &str) -> Command {
        let mut command = self.case.profile_command(source, "server-clock-status-v1");
        command
            .arg("--guest-request")
            .arg(&self.request.path)
            .arg("--guest-approval")
            .arg(&self.approval.path);
        command
    }
    pub fn spawn(&self, source: &str) -> Running {
        Running::start(self.command(source))
    }
    pub fn manifest(&self) -> Value {
        serde_json::from_slice(
            &fs::read(self.case.bundle.join("zryna-wasi-server-manifest-v2.json"))
                .expect("complete clock record"),
        )
        .expect("clock manifest")
    }
}
pub(super) fn document() -> Vec<u8> {
    serde_json::to_vec(&json!({"world":"zryna:capability-profiles/server@0.1.0",
        "requests":["clock"],"clock":{"monotonic_reads":1,"subscriptions":0,"timers":0}}))
    .expect("one-read guest request")
}
