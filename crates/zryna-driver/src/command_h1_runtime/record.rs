//! Closed non-secret observations retained after one consumed command store.

use serde::Serialize;

/// The exact observed outcome of one command invocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CommandH1Outcome {
    /// The canonical WIT run function returned its declared result.
    RunReturned {
        /// The declared `result<(), ()>` carrier, independent of process status.
        #[serde(rename = "runReturn")]
        result: CommandH1RunReturn,
    },
    /// A mediated host callback sealed the first denial before trapping.
    HostDenial {
        /// The exact authenticated imported interface.
        interface: String,
        /// The exact authenticated operation within that interface.
        operation: String,
    },
    /// Execution trapped without returning a WIT run result.
    RuntimeTrap {
        /// Classification derived from sealed denial state and audited runtime coordinates.
        category: CommandH1TrapCategory,
        /// Exact audited identity; absent for unrelated traps and process/host exceptions.
        #[serde(skip_serializing_if = "Option::is_none")]
        identity: Option<String>,
    },
}

/// The declared WIT result of an ordinary completed run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandH1RunReturn {
    /// Source `main` returned true.
    Ok,
    /// Source `main` returned false.
    Err,
}

/// Closed trap categories; these never imply a host denial.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandH1TrapCategory {
    /// The exact audited language run trap instruction executed.
    ControlledLanguage,
    /// The exact audited canonical storage guard trap instruction executed.
    InterfaceViolation,
    /// An unrelated raw trap, fuel/deadline exhaustion or host/process failure occurred.
    HostProcessFailure,
}

/// Independently observed store destruction and watchdog completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandH1Teardown {
    /// The store was consumed and the owned deadline worker joined successfully.
    Confirmed,
    /// Destruction or deadline-worker completion could not be confirmed.
    Unconfirmed,
}

/// Non-secret actual execution observations, without replay or grant authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CommandH1ExecutionRecord {
    outcome: CommandH1Outcome,
    teardown: CommandH1Teardown,
}

impl CommandH1ExecutionRecord {
    /// Returns the observed typed outcome.
    #[must_use]
    pub const fn outcome(&self) -> &CommandH1Outcome {
        &self.outcome
    }
    /// Returns the independently observed teardown status.
    #[must_use]
    pub const fn teardown(&self) -> CommandH1Teardown {
        self.teardown
    }
    /// Whether the run returned `ok(())` and teardown was confirmed.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        matches!(self.outcome, CommandH1Outcome::RunReturned { result: CommandH1RunReturn::Ok })
            && self.teardown == CommandH1Teardown::Confirmed
    }
    pub(super) const fn new(outcome: CommandH1Outcome, teardown: CommandH1Teardown) -> Self {
        Self { outcome, teardown }
    }
}
