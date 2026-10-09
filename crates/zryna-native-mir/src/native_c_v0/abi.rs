//! Physical claims for the admitted LP64 System V AMD64 INTEGER calling sequence.

use zryna_native_c_ir::contract::AbiType;

/// Ordered argument register, independent of backend register allocation for live values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Register {
    /// First INTEGER argument.
    Rdi,
    /// Second INTEGER argument.
    Rsi,
    /// Third INTEGER argument.
    Rdx,
    /// Fourth INTEGER argument.
    Rcx,
    /// Fifth INTEGER argument.
    R8,
    /// Sixth INTEGER argument.
    R9,
}
/// Argument lane location. Stack offsets start at zero in the outgoing area before call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Location {
    /// One INTEGER register.
    Register(Register),
    /// Distinct eight-byte outgoing slot; the return-address push is not part of this area.
    Stack(u32),
}
/// Exact argument spelling, meaningful width and physical location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lane {
    /// Declared spelling; C-int never silently becomes C-i32.
    pub abi: AbiType,
    /// Meaningful bits, with narrow excess bits ignored.
    pub bits: u8,
    /// Register or outgoing stack location.
    pub location: Location,
    /// Bool32 low-width value must be zero or one.
    pub canonical_bool: bool,
}
/// Exact low-width RAX result; no hidden aggregate return is admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResultLane {
    /// Declared spelling.
    pub abi: AbiType,
    /// Meaningful bits in RAX.
    pub bits: u8,
    /// Check only low 32 bits for the Bool32 0/1 domain.
    pub canonical_bool: bool,
}
/// Complete fixed C call signature, including independently verified stack padding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    /// Ordered INTEGER arguments.
    pub parameters: Vec<Lane>,
    /// Result in RAX, or no result for Unit.
    pub result: Option<ResultLane>,
    /// Eight-byte stack arguments plus zero padding to a multiple of sixteen.
    pub outgoing_bytes: u32,
    /// Required pre-call stack alignment.
    pub stack_alignment: u32,
}
/// Distinct caller-owned frame slot. Zeroing does not establish logical initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputSlot {
    /// Exact original output token.
    pub token: usize,
    /// Unique checked offset in the output-slot frame area.
    pub offset: u32,
    /// Four for i32, eight for a pointer or LP64 count.
    pub bytes: u32,
    /// Exact required alignment.
    pub alignment: u32,
    /// Physical zeroing precedes any C use.
    pub zeroed: bool,
    /// Must be false; initialization follows only the exact successful status call.
    pub initialized_at_entry: bool,
}
