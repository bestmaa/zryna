//! Untrusted straight-line scalar MIR claims, separate from every verified wrapper.

use super::MirType;

/// Untrusted function-local value identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueId(u32);

impl ValueId {
    /// Creates an unverified raw value identifier.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the claimed dense value index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Untrusted calling-convention claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallingConvention(u16);

impl CallingConvention {
    /// Provisional internal convention for the straight-line `i32` proof.
    ///
    /// This is not the public scalar ABI or an FFI contract.
    pub const ZRYNA_INTERNAL_I32_V1: Self = Self(1);

    /// Creates an unverified convention code for provider and negative-test inputs.
    #[must_use]
    pub const fn from_code(code: u16) -> Self {
        Self(code)
    }

    /// Returns the raw convention code.
    #[must_use]
    pub const fn code(self) -> u16 {
        self.0
    }
}

/// Untrusted native MIR function signature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    pub(super) parameters: Vec<MirType>,
    pub(super) result: MirType,
}

impl Signature {
    /// Creates an unverified signature claim.
    #[must_use]
    pub const fn new(parameters: Vec<MirType>, result: MirType) -> Self {
        Self { parameters, result }
    }
}

/// Untrusted native MIR operation claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Read a function argument.
    Parameter {
        /// Claimed zero-based parameter index.
        index: u32,
    },
    /// Create a signed 32-bit literal.
    I32Literal {
        /// Literal value.
        value: i32,
    },
    /// Add two signed 32-bit values with wrapping semantics.
    I32Add {
        /// Claimed left value.
        lhs: ValueId,
        /// Claimed right value.
        rhs: ValueId,
    },
}

/// Untrusted typed value definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueDefinition {
    pub(super) id: ValueId,
    pub(super) ty: MirType,
    pub(super) operation: Operation,
}

impl ValueDefinition {
    /// Creates an unverified typed value definition.
    #[must_use]
    pub const fn new(id: ValueId, ty: MirType, operation: Operation) -> Self {
        Self { id, ty, operation }
    }
}

/// Untrusted native MIR function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    pub(super) symbol: String,
    pub(super) convention: CallingConvention,
    pub(super) signature: Signature,
    pub(super) values: Vec<ValueDefinition>,
    pub(super) result: ValueId,
}

impl Function {
    /// Creates an unverified native function claim.
    #[must_use]
    pub const fn new(
        symbol: String,
        convention: CallingConvention,
        signature: Signature,
        values: Vec<ValueDefinition>,
        result: ValueId,
    ) -> Self {
        Self { symbol, convention, signature, values, result }
    }
}

/// Untrusted native MIR module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    pub(super) functions: Vec<Function>,
}

impl Module {
    /// Creates an unverified native module claim.
    #[must_use]
    pub const fn new(functions: Vec<Function>) -> Self {
        Self { functions }
    }
}
