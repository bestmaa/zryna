//! Canonical fail-closed Zryna repository architecture engine.

#![forbid(unsafe_code)]

mod cargo;
mod components;
mod contract;
mod dependency_graph;
mod diagnostics;
mod filesystem;
mod validation;

pub use contract::{AdapterContract, MemberContract, MemberKind, WorkspaceContract};
pub use diagnostics::ValidationReport;
pub use validation::validate_workspace;

#[cfg(test)]
mod tests;
