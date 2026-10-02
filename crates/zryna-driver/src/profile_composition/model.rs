use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub(super) const VERSION: &str = "zryna.cross-target-profiles.v1";
pub(super) const POLICY: &str = "zryna.wit-capability-profiles.v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(super) enum Language {
    I32V1,
    ControlFlowV1,
    DataOwnershipV1,
    CommandH1V1,
}

// Declaration vocabulary only. Actual source admission remains with the corresponding IR verifier.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(super) enum Row {
    UniversalJavaScript,
    JavaScriptBrowser,
    JavaScriptNode,
    UniversalWebAssembly,
    WitBrowser,
    WitCommand,
    WitServer,
    UniversalNative,
    NativeHost,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(super) enum Capability {
    Clock,
    Environment,
    Filesystem,
    Network,
    Randomness,
}

impl Capability {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Clock => "clock",
            Self::Environment => "environment",
            Self::Filesystem => "filesystem",
            Self::Network => "network",
            Self::Randomness => "randomness",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Requirement {
    pub capability: Capability,
    pub interface: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Reservation {
    pub subscriptions: u64,
    pub timers: u64,
    pub environment: BTreeMap<String, String>,
    pub preopens: BTreeSet<String>,
    pub descriptors: u64,
    pub endpoints: BTreeSet<String>,
    pub operations: u64,
    pub random_per_call: u64,
    pub random_total: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Instance {
    pub id: String,
    pub rows: BTreeSet<Row>,
    pub requirements: BTreeSet<Requirement>,
    pub restrictions: BTreeSet<Capability>,
    pub reservation: Reservation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Selection {
    pub row: Row,
    pub policy_version: String,
    pub world: Option<String>,
    pub approved: BTreeSet<Requirement>,
    // Host-owned narrowing for each of the ten #167 metrics; never a grant.
    pub ceilings: [u64; 10],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    pub version: String,
    pub root: String,
    pub language: Language,
    pub selections: Vec<Selection>,
    pub instances: Vec<Instance>,
    pub edges: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Summary {
    pub requirements: BTreeSet<Requirement>,
    pub quota: [u64; 10],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Claim {
    pub binding: [u8; 32],
    pub summaries: BTreeMap<String, Summary>,
    pub witnesses: BTreeMap<Requirement, Vec<String>>,
}
