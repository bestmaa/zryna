//! Untrusted additive owned operations and complete cleanup claims; no executable authority.
use crate::generic_v1::raw as graph;
/// Separate owned wire claim. Core placeholders grant no old Copy-lane authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    /// Exact closed inventory, signatures and CFG.
    pub graph: graph::Program,
    /// One complete ordered extension list per canonical function.
    pub extensions: Vec<Vec<Extension>>,
    /// One independently checked cleanup plan per canonical function.
    pub plans: Vec<Plan>,
}
/// Extended instruction identified by its unique result definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Extension {
    /// Exact result ID of the replaced Unit placeholder.
    pub result: u32,
    /// Separate owned opcode; selected shared String clone requires private v3 transport.
    pub operation: Operation,
}
/// Separate owned operation vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// UTF-8 copied into a fresh owned runtime String.
    StringLiteral(Vec<u8>),
    /// Whole-value transfer, never implicit cloning.
    Move(u32),
    /// Loan of one complete live owner.
    Borrow {
        /// Exact live stored value.
        value: u32,
        /// Exclusive rather than shared loan.
        exclusive: bool,
    },
    /// End an exact local loan.
    EndLoan(u32),
    /// Destroy an exact live value and its active payload recursively.
    Drop(u32),
    /// Explicit String clone, retaining its source on failure.
    CloneString(u32),
    /// Compiler-generated selected payload clone through a live shared String loan.
    /// Encoded only in the distinct private v3 domain.
    CloneBorrowedString(u32),
}
/// Complete instruction and terminator cleanup records.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Plan {
    /// Canonical block/position order.
    pub steps: Vec<Step>,
}
/// Exact cleanup at a fallible instruction or successful return.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Step {
    /// Block index.
    pub block: u32,
    /// Instruction index, or instruction count for the terminator.
    pub position: u32,
    /// Loans ended before cleanup.
    pub end_loans: Vec<u32>,
    /// Live complete owners in reverse creation/transfer order.
    pub cleanup: Vec<u32>,
    /// Failure-only cleanup, otherwise successful return cleanup.
    pub failure: bool,
}
