//! Untrusted successor graph vocabulary; separate wire admission still grants no executable authority.

use zryna_source::UntrustedSpan;

/// One claimed complete module inventory entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    /// Dense original source file/module index.
    pub id: u32,
    /// Original source-ordered function count, including unused templates.
    pub functions: u32,
}

/// Claimed original function metadata, independently checked by `validate_source_graph`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Declaration {
    /// Original module index.
    pub module: u32,
    /// Original function source index.
    pub function: u32,
    /// Zero, one or two bounded type parameters.
    pub parameters: u32,
    /// Original declaration range.
    pub span: UntrustedSpan,
}

/// Exact logical value type; only stored types enter successor layout keys.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Type {
    /// Exact canonical successor layout type index.
    Stored(u32),
    /// An ordinary unit result, never a generic type argument or stored payload.
    Unit,
    /// A nonstored loan with exact referent and mutability.
    Borrow {
        /// Exact stored referent.
        referent: u32,
        /// Exclusive rather than shared loan.
        exclusive: bool,
    },
}

/// One claimed value definition in the function's dense value arena.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Definition {
    /// Function-global dense value index.
    pub id: u32,
    /// Exact claimed type.
    pub ty: Type,
}

/// Closed typed operations. Ownership and source authentication are separate mandatory checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Fixed bool literal.
    BoolLiteral(bool),
    /// Fixed i32 literal.
    I32Literal(i32),
    /// Explicit unit result.
    Unit,
    /// Copy observation; rejected for any owned layout.
    Copy {
        /// Exact earlier/dominating value.
        value: u32,
    },
    /// Exact wrapping scalar addition, never an operation on an opaque parameter.
    I32Add {
        /// Earlier left operand.
        left: u32,
        /// Earlier right operand.
        right: u32,
    },
    /// Logical `ClosedGenericCall` with a dense unsigned-key-ordered generic instance index.
    ClosedGenericCall {
        /// Claimed existing closed generic instance index.
        instance: u32,
        /// Source-evaluation-ordered value operands.
        arguments: Vec<u32>,
    },
    /// Direct call to one nongeneric source root, without a monomorphization index.
    SourceCall {
        /// Original module index.
        module: u32,
        /// Original function source index.
        function: u32,
        /// Source-evaluation-ordered value operands.
        arguments: Vec<u32>,
    },
    /// Logical `ClosedEnumConstruct`, with exactly zero or one active payload operand.
    ClosedEnumConstruct {
        /// Complete closed family/nominal enum type index.
        ty: u32,
        /// Fixed source/family ordinal.
        ordinal: u32,
        /// Exactly one payload operand for a payload variant; absent otherwise.
        payload: Option<u32>,
    },
}

/// One typed instruction with exact original range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instruction {
    /// One result definition.
    pub result: Definition,
    /// Claimed source range.
    pub span: UntrustedSpan,
    /// Exact closed operation.
    pub operation: Operation,
}

/// An explicit ordinary CFG edge; parameters retain exact source evaluation order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Edge {
    /// Dense target block index.
    pub target: u32,
    /// Existing value operands for the target parameters.
    pub arguments: Vec<u32>,
}

/// Closed logical match mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchMode {
    /// Copy observation or owned scrutinee consumption.
    Value,
    /// Retained shared loan with borrowed active payload.
    SharedBorrow,
    /// Retained exclusive loan with exclusively borrowed active payload.
    ExclusiveBorrow,
}

/// One exact ordinal match successor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Arm {
    /// Fixed ordinal, in canonical order.
    pub ordinal: u32,
    /// Generated active payload definition, equal to the target's first parameter if present.
    pub binding: Option<Definition>,
    /// Remaining ordinary target arguments, excluding the active binding.
    pub edge: Edge,
}

/// Exactly one explicit terminator per block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Terminator {
    /// Exact function result.
    Return(u32),
    /// Ordinary edge.
    Jump(Edge),
    /// Exact bool conditional; only the selected edge executes.
    Branch {
        /// Existing bool value.
        condition: u32,
        /// True edge.
        yes: Edge,
        /// False edge.
        no: Edge,
    },
    /// Logical `ClosedEnumMatch`, with one successor for every fixed variant.
    ClosedEnumMatch {
        /// Exact closed enum family/type index.
        ty: u32,
        /// Existing scrutinee, evaluated once by its producer.
        scrutinee: u32,
        /// Exact value/loan mode.
        mode: MatchMode,
        /// Exact exhaustive ordinal-ordered successors.
        arms: Vec<Arm>,
    },
}

/// One dense claimed basic block, including every claimed unreachable block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    /// Dense source-independent block index; entry is zero.
    pub id: u32,
    /// Dense value definitions for explicit incoming parameters.
    pub parameters: Vec<Definition>,
    /// Source-evaluation-ordered instructions.
    pub instructions: Vec<Instruction>,
    /// Source range for the terminator.
    pub span: UntrustedSpan,
    /// Exactly one explicit terminator.
    pub terminator: Terminator,
}

/// One untrusted complete closed function/root claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    /// Complete tag-40 instance or tag-41 root key; records are unsigned-key sorted.
    pub key: Vec<u8>,
    /// Exact original declaration range, not a new synthesized range.
    pub span: UntrustedSpan,
    /// Claimed public scalar export; never permitted on a generic instance.
    pub public_export: Option<String>,
    /// Substituted exact value parameter types.
    pub parameters: Vec<Type>,
    /// Substituted exact result type.
    pub result: Type,
    /// Dense complete block inventory, entry first.
    pub blocks: Vec<Block>,
}

/// Complete raw successor claims; there is no backend-consumable authority here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    /// Complete final module inventory.
    pub modules: Vec<Module>,
    /// Every original source function, including uninstantiated templates.
    pub declarations: Vec<Declaration>,
    /// Complete canonical successor type keys, in assigned layout-ID order.
    pub type_keys: Vec<Vec<u8>>,
    /// Claimed target-neutral complete universe identity.
    pub universe: [u8; 32],
    /// Claimed exact `Linear32V1` layout fingerprint.
    pub linear32: [u8; 32],
    /// Claimed exact `LinuxX8664V1` layout fingerprint.
    pub linux_x86_64: [u8; 32],
    /// Complete demanded closed functions plus every nongeneric source root.
    pub functions: Vec<Function>,
}
