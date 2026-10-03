use super::super::{DeclarationIdentity, TypeParameterIdentity};

/// Origins are indices into the retained, authenticated source arenas. They are not type IDs
/// for a closed program. In particular, an arm origin is not a loan or region identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Origin {
    Source { module: u32, occurrence: u32 },
    Expression { function: DeclarationIdentity, expression: u32 },
    Statement { function: DeclarationIdentity, statement: u32 },
    ArmBorrow { function: DeclarationIdentity, expression: u32, arm: u32 },
    Scalar(Scalar),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Scalar {
    Bool,
    I32,
    String,
    Unit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Ty {
    pub(super) origin: Origin,
    /// Zero is the identity environment. Other indices are local to the current function.
    pub(super) environment: u32,
}

impl Ty {
    pub(super) const fn source(module: u32, occurrence: u32) -> Self {
        Self { origin: Origin::Source { module, occurrence }, environment: 0 }
    }

    pub(super) const fn scalar(scalar: Scalar) -> Self {
        Self { origin: Origin::Scalar(scalar), environment: 0 }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Scalar(Scalar),
    Parameter(TypeParameterIdentity),
    Nominal(DeclarationIdentity),
    Option,
    Result,
    Vec,
    Shared,
    Weak,
    Borrow,
    BorrowMut,
    FixedArray(u32),
    Function(DeclarationIdentity),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Head {
    pub(super) kind: Kind,
    pub(super) children: [Option<Ty>; 2],
}

impl Head {
    pub(super) const fn leaf(kind: Kind) -> Self {
        Self { kind, children: [None, None] }
    }

    pub(super) const fn unary(kind: Kind, child: Ty) -> Self {
        Self { kind, children: [Some(child), None] }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SourceRecord {
    pub(super) owner: DeclarationIdentity,
    /// An invalid use never has a head. It cannot become equal to another invalid use.
    pub(super) head: Option<Head>,
    pub(super) invalid_arguments: bool,
    pub(super) argument_occurrence: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ResultRecord {
    pub(super) ty: Option<Ty>,
    pub(super) head: Option<Head>,
    pub(super) rank: u32,
    pub(super) arm_start: usize,
    pub(super) match_valid: bool,
    pub(super) match_generic: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ArmRecord {
    pub(super) origin: Origin,
    pub(super) payload: Option<Ty>,
    pub(super) head: Option<Head>,
    pub(super) rank: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Environment {
    pub(super) owner: DeclarationIdentity,
    pub(super) arguments: [Option<Ty>; 2],
    pub(super) capabilities: [[Status; 4]; 2],
}

#[derive(Debug)]
pub(super) struct FunctionRecords {
    pub(super) owner: DeclarationIdentity,
    pub(super) expressions: Vec<ResultRecord>,
    pub(super) statements: Vec<ResultRecord>,
    pub(super) arms: Vec<ArmRecord>,
    pub(super) environments: Vec<Environment>,
    pub(super) next_rank: u32,
}

#[derive(Debug)]
pub(super) struct Tables {
    pub(super) sources: Vec<Vec<SourceRecord>>,
    pub(super) functions: Vec<FunctionRecords>,
    pub(super) function_offsets: Vec<usize>,
}

impl Tables {
    pub(super) fn function(&self, owner: DeclarationIdentity) -> &FunctionRecords {
        let index =
            self.function_offsets[owner.module().index() as usize] + owner.source_index() as usize;
        let records = &self.functions[index];
        assert_eq!(records.owner, owner, "issuing original function");
        records
    }

    pub(super) fn function_mut(&mut self, owner: DeclarationIdentity) -> &mut FunctionRecords {
        let index =
            self.function_offsets[owner.module().index() as usize] + owner.source_index() as usize;
        let records = &mut self.functions[index];
        assert_eq!(records.owner, owner, "issuing original function");
        records
    }
}

// Two status planes encode the 32 original predicate outputs without a normalized type table.
use super::capabilities::Status;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CapabilityStatuses {
    true_bits: u32,
    unknown_bits: u32,
}

impl CapabilityStatuses {
    pub(super) const TRUE: Self = Self { true_bits: u32::MAX, unknown_bits: 0 };
    pub(super) fn get(self, output: usize) -> Status {
        if self.true_bits & (1 << output) != 0 {
            Status::True
        } else if self.unknown_bits & (1 << output) != 0 {
            Status::Unknown
        } else {
            Status::False
        }
    }
    pub(super) fn from(values: [Status; 32]) -> Self {
        let mut result = Self { true_bits: 0, unknown_bits: 0 };
        for (output, value) in values.into_iter().enumerate() {
            match value {
                Status::True => result.true_bits |= 1 << output,
                Status::Unknown => result.unknown_bits |= 1 << output,
                Status::False => {}
            }
        }
        result
    }
}
