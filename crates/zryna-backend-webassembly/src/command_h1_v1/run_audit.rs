//! Independently checked run cleanup and exact controlled-trap instruction locations.

use wasmparser::{BlockType, FunctionBody, Operator, ValType};
use zryna_diagnostics::Diagnostic;
use zryna_ir::data_ownership_v1::VerifiedTrapIdentity;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// An independently audited controlled-language trap location, without runtime authority.
pub struct TrapSite {
    pub(super) function_index: u32,
    pub(super) module_offset: u64,
    pub(super) identity: VerifiedTrapIdentity,
}

impl TrapSite {
    /// Returns the language core function index.
    #[must_use]
    pub const fn function_index(self) -> u32 {
        self.function_index
    }

    /// Returns the exact component-relative opcode offset.
    #[must_use]
    pub const fn module_offset(self) -> u64 {
        self.module_offset
    }

    /// Returns the verified identity assigned to this exact controlled trap instruction.
    #[must_use]
    pub const fn identity(self) -> VerifiedTrapIdentity {
        self.identity
    }
}

enum Step {
    Call(u32),
    Constant(i32),
    ReadGlobal(u32),
    WriteGlobal(u32),
    ReadLocal(u32),
    WriteLocal(u32),
    Equal,
    EqualZero,
    If,
    End,
    Trap(Option<VerifiedTrapIdentity>),
}

/// Checks the complete instruction sequence, rather than trusting a trap name or opcode alone.
pub(super) fn audit(
    body: &FunctionBody<'_>,
    function_index: u32,
    main_index: u32,
    clone_frontier: bool,
) -> Result<Vec<TrapSite>, Diagnostic> {
    use Step::{Call, Constant, End, Equal, EqualZero, If, ReadGlobal, ReadLocal, Trap};
    use Step::{WriteGlobal, WriteLocal};
    let mut locals = body.get_locals_reader().map_err(|_| invalid())?;
    if locals.get_count() != 1 || locals.read().map_err(|_| invalid())? != (1, ValType::I32) {
        return Err(invalid());
    }
    // Entry drains stale canonical temporaries before resetting only language state.
    // The second drain and language rewind precede every controlled trap branch.
    let mut steps = vec![
        Call(4),
        Constant(0),
        WriteGlobal(1),
        Constant(0),
        WriteGlobal(2),
        Constant(0),
        WriteGlobal(5),
    ];
    if clone_frontier {
        steps.extend([Constant(0), WriteGlobal(9)]);
    }
    steps.extend([Constant(65_536), WriteGlobal(0), Call(main_index), EqualZero, WriteLocal(0)]);
    if clone_frontier {
        steps.extend([Constant(0), WriteGlobal(9)]);
    }
    steps.extend([Call(4), Constant(65_536), WriteGlobal(0)]);
    // The shared core uses its private observation representation 1..5. These
    // mappings are deliberately distinct from ownership-runtime ABI statuses.
    for (code, identity) in [
        (1, VerifiedTrapIdentity::BoundsV1),
        (2, VerifiedTrapIdentity::AllocationV1),
        (3, VerifiedTrapIdentity::CapacityV1),
        (4, VerifiedTrapIdentity::RefcountV1),
        (5, VerifiedTrapIdentity::Utf8V1),
    ] {
        steps.extend([ReadGlobal(1), Constant(code), Equal, If, Trap(Some(identity)), End]);
    }
    // Unknown status traps without inventing a controlled-language identity.
    steps.extend([ReadGlobal(1), If, Trap(None), End, ReadLocal(0), End]);
    let mut operators = body.get_operators_reader().map_err(|_| invalid())?;
    let mut sites = Vec::new();
    for step in steps {
        let (operator, offset) = operators.read_with_offset().map_err(|_| invalid())?;
        if !matches_step(&step, &operator) {
            return Err(invalid());
        }
        if let Trap(Some(identity)) = step {
            sites.push(TrapSite { function_index, module_offset: offset, identity });
        }
    }
    if !operators.eof() {
        return Err(invalid());
    }
    Ok(sites)
}

fn matches_step(step: &Step, operator: &Operator<'_>) -> bool {
    match (step, operator) {
        (Step::Call(expected), Operator::Call { function_index }) => expected == function_index,
        (Step::Constant(expected), Operator::I32Const { value }) => expected == value,
        (Step::ReadGlobal(expected), Operator::GlobalGet { global_index })
        | (Step::WriteGlobal(expected), Operator::GlobalSet { global_index }) => {
            expected == global_index
        }
        (Step::ReadLocal(expected), Operator::LocalGet { local_index })
        | (Step::WriteLocal(expected), Operator::LocalSet { local_index }) => {
            expected == local_index
        }
        (Step::Equal, Operator::I32Eq)
        | (Step::EqualZero, Operator::I32Eqz)
        | (Step::If, Operator::If { blockty: BlockType::Empty })
        | (Step::End, Operator::End)
        | (Step::Trap(_), Operator::Unreachable) => true,
        _ => false,
    }
}

fn invalid() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-W4103",
        None,
        "Command run differs from its verified entry, cleanup or trap mapping.",
        "Emit the complete command body and audit its exact run instruction sequence.",
    )
}
