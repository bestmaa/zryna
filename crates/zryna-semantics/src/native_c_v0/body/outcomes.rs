//! Required boundary checks and closed future failure routes; no runtime execution proof.

/// Required runtime boundary action; its presence is not proof that a runtime performed it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundaryCheck {
    /// Check a signed count before explicit conversion to the selected C `size_t` lane.
    CountConversion {
        /// Original argument supplying the signed count.
        expression: usize,
    },
    /// Normalize/check the complete canonical Boolean carrier at the raw ABI boundary.
    BooleanCarrier {
        /// Original Boolean argument expression.
        expression: usize,
    },
    /// Validate returned Boolean bits before exposing a language Boolean value.
    BooleanResult,
    /// Replay borrowed pointer/count bounds from the exact declared resource group.
    Borrow {
        /// Exact resource-group ordinal in the retained operation.
        group: usize,
        /// Scoped pointer/backing-storage token identity.
        loan: usize,
        /// Original count argument whose value must fit that backing length.
        count_expression: usize,
        /// Exact loan identity when the count is an unchanged byte-length expression.
        length_origin: Option<usize>,
    },
}

/// Fixed trap requirement for a future runtime action, without inventing an M3 trap identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrapRequirement {
    /// Checked wrapper length fails before C entry with `FOREIGN_LENGTH`.
    ForeignLength,
    /// Checked vector element conversion fails before C entry with `FOREIGN_BYTE_RANGE`.
    ForeignByteRange,
    /// Shared execution-instance reservation fails before acquisition with `FOREIGN_RESOURCE_LIMIT`.
    ForeignResourceLimit,
    /// Keep the exact trap issued by the later independently authenticated private preparation.
    PreservePrivatePreparationIdentity,
}

/// Source-derived C-entry condition, separate from logical token consumption.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallEntry {
    /// Raw entry subject to the retained declaration's independent ABI checks.
    Always,
    /// Safe byte release skips C for a canonical empty owner while still consuming its token.
    NonEmptyOwner(usize),
}

/// Required terminal classification for a future independently verified runtime edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureRoute {
    /// Only a member of the exact creating call's declared recoverable status domain.
    DeclaredForeignError,
    /// Unknown status or invalid output is a host/ABI failure, never a language trap.
    HostAbiFailure,
    /// A release defect overrides a prior return/error/trap and leaves cleanup unresolved.
    ReleaseFailureOverridesUnresolved,
    /// A C process fault has no promised in-process recovery or guaranteed cleanup.
    ProcessFailureNoCleanupGuarantee,
}
