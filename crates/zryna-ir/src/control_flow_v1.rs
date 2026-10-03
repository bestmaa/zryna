//! Verified structured control-flow IR for the `ControlFlowV1` profile.
//!
//! This module is deliberately separate from the stable [`crate::VerifiedProgram`] M1 surface.
//! Values in [`crate::control_flow_v1::raw`] are untrusted claims. Only [`verify`] can construct the
//! opaque verified views exposed here, and none of those views provides access to the raw program.

use std::collections::{BTreeMap, VecDeque};

use zryna_abi::{AbiViolationKind, VerifiedScalarExport, raw as raw_abi, verify_v1};
use zryna_diagnostics::Diagnostic;
use zryna_source::{FileId, SourceMap, Span};

use crate::Type;

/// Maximum modules in one verified program.
pub const MAX_MODULES: usize = 4_096;
/// Maximum functions in one module.
pub const MAX_FUNCTIONS_PER_MODULE: usize = 4_096;
/// Maximum functions in one program.
pub const MAX_FUNCTIONS_PER_PROGRAM: usize = 16_384;
/// Maximum parameters in one function.
pub const MAX_PARAMETERS_PER_FUNCTION: usize = 256;
/// Maximum parameters in one program.
pub const MAX_PARAMETERS_PER_PROGRAM: usize = 262_144;
/// Maximum blocks in one function.
pub const MAX_BLOCKS_PER_FUNCTION: usize = 4_096;
/// Maximum blocks in one program.
pub const MAX_BLOCKS_PER_PROGRAM: usize = 65_536;
/// Maximum block parameters on one block.
pub const MAX_BLOCK_PARAMETERS: usize = 256;
/// Maximum values in one function.
pub const MAX_VALUES_PER_FUNCTION: usize = 16_384;
/// Maximum values in one program.
pub const MAX_VALUES_PER_PROGRAM: usize = 262_144;
/// Maximum CFG edges in one function.
pub const MAX_CFG_EDGES_PER_FUNCTION: usize = 8_192;
/// Maximum CFG edges in one program.
pub const MAX_CFG_EDGES_PER_PROGRAM: usize = 131_072;
/// Maximum direct-call sites in one program.
pub const MAX_CALL_EDGES: usize = 65_536;
/// Maximum static direct-call depth.
pub const MAX_STATIC_CALL_DEPTH: usize = 128;
/// Maximum verified natural-loop nesting.
pub const MAX_LOOP_NESTING: usize = 128;
/// Maximum retained diagnostics, including the terminal diagnostic.
pub const MAX_DIAGNOSTICS: usize = 256;

/// Untrusted structured IR claims created by semantic lowering.
pub mod raw {
    use serde::Serialize;
    use zryna_source::{FileId, Span};

    use crate::Type;

    /// Claimed dense module identifier.
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct ModuleId(pub u32);

    /// Claimed canonical function identity.
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct FunctionId {
        /// Dense containing module identifier.
        pub module: ModuleId,
        /// Source declaration index inside the module.
        pub declaration: u32,
    }

    /// Claimed dense block identifier local to one function.
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct BlockId(pub u32);

    /// Claimed dense value identifier local to one function.
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct ValueId(pub u32);

    /// One claimed typed value definition.
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    #[serde(deny_unknown_fields)]
    pub struct ValueDefinition {
        /// Claimed dense value identifier.
        pub id: ValueId,
        /// Claimed exact scalar type.
        pub ty: Type,
        /// Authoritative source range claim.
        pub span: Span,
    }

    /// One claimed typed instruction result.
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    #[serde(deny_unknown_fields)]
    pub struct Instruction {
        /// Claimed result definition.
        pub result: ValueDefinition,
        /// Claimed operation.
        pub kind: InstructionKind,
    }

    /// Exhaustive operations in `ControlFlowV1`.
    #[allow(missing_docs)]
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum InstructionKind {
        /// Boolean literal.
        BoolLiteral(bool),
        /// Signed 32-bit integer literal.
        I32Literal(i32),
        /// Wrapping signed 32-bit addition.
        I32Add { lhs: ValueId, rhs: ValueId },
        /// Wrapping signed 32-bit subtraction.
        I32Sub { lhs: ValueId, rhs: ValueId },
        /// Low-32-bit signed multiplication.
        I32Mul { lhs: ValueId, rhs: ValueId },
        /// Wrapping signed negation.
        I32Neg { operand: ValueId },
        /// Exact same-scalar equality.
        Eq { lhs: ValueId, rhs: ValueId },
        /// Exact same-scalar inequality.
        Ne { lhs: ValueId, rhs: ValueId },
        /// Signed less-than.
        I32LtS { lhs: ValueId, rhs: ValueId },
        /// Signed less-than-or-equal.
        I32LeS { lhs: ValueId, rhs: ValueId },
        /// Signed greater-than.
        I32GtS { lhs: ValueId, rhs: ValueId },
        /// Signed greater-than-or-equal.
        I32GeS { lhs: ValueId, rhs: ValueId },
        /// Statically resolved direct call.
        DirectCall { callee: FunctionId, arguments: Vec<ValueId> },
    }

    /// One claimed block terminator with its source range.
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    #[serde(deny_unknown_fields)]
    pub struct SpannedTerminator {
        /// Source range of the terminating statement.
        pub span: Span,
        /// Claimed terminator.
        pub kind: Terminator,
    }

    /// Exhaustive terminators in `ControlFlowV1`.
    #[allow(missing_docs)]
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum Terminator {
        /// Return one scalar value.
        Return(ValueId),
        /// Transfer control with exact block arguments.
        Jump { target: BlockId, arguments: Vec<ValueId> },
        /// Select one of two explicit edges.
        Branch {
            condition: ValueId,
            true_target: BlockId,
            true_arguments: Vec<ValueId>,
            false_target: BlockId,
            false_arguments: Vec<ValueId>,
        },
    }

    /// One claimed dense basic block.
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    #[serde(deny_unknown_fields)]
    pub struct Block {
        /// Claimed dense block identifier.
        pub id: BlockId,
        /// Block parameters, empty for entry block zero.
        pub parameters: Vec<ValueDefinition>,
        /// Ordered instruction results.
        pub instructions: Vec<Instruction>,
        /// Must contain exactly one terminator; the vector preserves invalid claims for rejection.
        pub terminators: Vec<SpannedTerminator>,
    }

    /// One claimed function in source declaration order.
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    #[serde(deny_unknown_fields)]
    pub struct Function {
        /// Claimed canonical function identity.
        pub id: FunctionId,
        /// Public scalar ABI name for an entry-module export, otherwise `None`.
        pub entry_export: Option<String>,
        /// Function declaration source range.
        pub span: Span,
        /// Function parameters in signature order.
        pub parameters: Vec<ValueDefinition>,
        /// Exact result type.
        pub result: Type,
        /// Dense block arena; block zero is entry.
        pub blocks: Vec<Block>,
    }

    /// One claimed module ordered by normalized source path.
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    #[serde(deny_unknown_fields)]
    pub struct Module {
        /// Claimed dense module identifier.
        pub id: ModuleId,
        /// Exact source-map file authority for this module.
        pub source_file: FileId,
        /// Functions in source declaration order.
        pub functions: Vec<Function>,
    }

    /// Complete raw `ControlFlowV1` program.
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    #[serde(deny_unknown_fields)]
    pub struct Program {
        /// Claimed entry module. Verification binds it to an independent expected entry file.
        pub entry_module: ModuleId,
        /// Complete module closure in normalized path byte order.
        pub modules: Vec<Module>,
    }
}

/// Opaque verified module identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ModuleIdentity(u32);

impl ModuleIdentity {
    /// Returns the dense module index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Opaque verified function identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FunctionIdentity {
    module: ModuleIdentity,
    declaration: u32,
}

impl FunctionIdentity {
    /// Returns the containing verified module identity.
    #[must_use]
    pub const fn module(self) -> ModuleIdentity {
        self.module
    }
    /// Returns the source declaration index.
    #[must_use]
    pub const fn declaration(self) -> u32 {
        self.declaration
    }
}

/// Opaque verified block identity within one function.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BlockIdentity(u32);

impl BlockIdentity {
    /// Returns the dense block index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Opaque verified value identity within one function.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ValueIdentity(u32);

impl ValueIdentity {
    /// Returns the dense value index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Program proven to satisfy every `ControlFlowV1` IR invariant.
///
/// Construction and raw recovery are intentionally unavailable:
///
/// ```compile_fail
/// let raw = zryna_ir::control_flow_v1::raw::Program {
///     entry_module: zryna_ir::control_flow_v1::raw::ModuleId(0),
///     modules: Vec::new(),
/// };
/// let _ = zryna_ir::control_flow_v1::VerifiedProgram { program: raw };
/// ```
///
/// ```compile_fail
/// fn recover_raw(program: &zryna_ir::control_flow_v1::VerifiedProgram) {
///     let _ = &program.program;
/// }
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedProgram {
    program: raw::Program,
    abi: zryna_abi::VerifiedScalarAbiModule,
    abi_indices: Vec<Vec<Option<usize>>>,
}

impl VerifiedProgram {
    /// Returns the independently verified entry module identity.
    #[must_use]
    pub const fn entry_module(&self) -> ModuleIdentity {
        ModuleIdentity(self.program.entry_module.0)
    }

    /// Iterates immutable verified modules in canonical order.
    #[must_use]
    pub fn modules(&self) -> impl ExactSizeIterator<Item = VerifiedModule<'_>> {
        self.program.modules.iter().enumerate().map(|(index, module)| VerifiedModule {
            owner: self,
            index,
            module,
        })
    }

    /// Returns the sealed public scalar ABI for entry-module exports only.
    #[must_use]
    pub const fn scalar_abi(&self) -> &zryna_abi::VerifiedScalarAbiModule {
        &self.abi
    }

    /// Validates one typed public invocation against the sealed ABI.
    ///
    /// # Errors
    ///
    /// Rejects an unknown export, wrong arity, or mismatched typed argument.
    pub fn prepare_invocation(
        &self,
        invocation: zryna_abi::Invocation,
    ) -> Result<zryna_abi::VerifiedInvocation<'_>, zryna_abi::InvocationError> {
        self.abi.prepare_invocation(invocation)
    }
}

/// Immutable verified module view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedModule<'program> {
    owner: &'program VerifiedProgram,
    index: usize,
    module: &'program raw::Module,
}

impl<'program> VerifiedModule<'program> {
    /// Returns the sealed module identity.
    #[must_use]
    pub const fn id(self) -> ModuleIdentity {
        ModuleIdentity(self.module.id.0)
    }
    /// Returns the exact source-map file authority.
    #[must_use]
    pub const fn source_file(self) -> FileId {
        self.module.source_file
    }
    /// Iterates verified functions in declaration order.
    #[must_use]
    pub fn functions(self) -> impl ExactSizeIterator<Item = VerifiedFunction<'program>> {
        self.module.functions.iter().enumerate().map(move |(function_index, function)| {
            VerifiedFunction {
                owner: self.owner,
                module_index: self.index,
                function_index,
                function,
            }
        })
    }
}

/// Immutable verified function view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedFunction<'program> {
    owner: &'program VerifiedProgram,
    module_index: usize,
    function_index: usize,
    function: &'program raw::Function,
}

impl<'program> VerifiedFunction<'program> {
    /// Returns the sealed canonical identity.
    #[must_use]
    pub const fn id(self) -> FunctionIdentity {
        FunctionIdentity {
            module: ModuleIdentity(self.function.id.module.0),
            declaration: self.function.id.declaration,
        }
    }
    /// Returns the verified public ABI export, only for entry exports.
    #[must_use]
    pub fn public_export(self) -> Option<VerifiedScalarExport<'program>> {
        let index = self.owner.abi_indices[self.module_index][self.function_index]?;
        self.owner.abi.exports().nth(index)
    }
    /// Returns exact parameter types in source order.
    #[must_use]
    pub fn parameters(
        self,
    ) -> impl ExactSizeIterator<Item = (ValueIdentity, Type, Span)> + 'program {
        self.function
            .parameters
            .iter()
            .map(|value| (ValueIdentity(value.id.0), value.ty, value.span))
    }
    /// Returns the exact result type.
    #[must_use]
    pub const fn result(self) -> Type {
        self.function.result
    }
    /// Returns the declaration span.
    #[must_use]
    pub const fn span(self) -> Span {
        self.function.span
    }
    /// Iterates verified blocks in dense order.
    #[must_use]
    pub fn blocks(self) -> impl ExactSizeIterator<Item = VerifiedBlock<'program>> {
        self.function.blocks.iter().map(move |block| VerifiedBlock { function: self, block })
    }
}

/// Immutable verified block view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedBlock<'program> {
    function: VerifiedFunction<'program>,
    block: &'program raw::Block,
}

impl<'program> VerifiedBlock<'program> {
    /// Returns the sealed block identity.
    #[must_use]
    pub const fn id(self) -> BlockIdentity {
        BlockIdentity(self.block.id.0)
    }
    /// Iterates verified block parameters.
    #[must_use]
    pub fn parameters(
        self,
    ) -> impl ExactSizeIterator<Item = (ValueIdentity, Type, Span)> + 'program {
        self.block.parameters.iter().map(|value| (ValueIdentity(value.id.0), value.ty, value.span))
    }
    /// Iterates verified instructions.
    #[must_use]
    pub fn instructions(self) -> impl ExactSizeIterator<Item = VerifiedInstruction<'program>> {
        self.block
            .instructions
            .iter()
            .map(move |instruction| VerifiedInstruction { function: self.function, instruction })
    }
    /// Returns the exactly-one verified terminator.
    #[must_use]
    pub fn terminator(self) -> VerifiedTerminator<'program> {
        VerifiedTerminator { function: self.function, terminator: &self.block.terminators[0] }
    }
}

/// Immutable verified instruction view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedInstruction<'program> {
    function: VerifiedFunction<'program>,
    instruction: &'program raw::Instruction,
}

impl<'program> VerifiedInstruction<'program> {
    /// Returns the sealed result identity.
    #[must_use]
    pub const fn result(self) -> ValueIdentity {
        ValueIdentity(self.instruction.result.id.0)
    }
    /// Returns the verified result type.
    #[must_use]
    pub const fn ty(self) -> Type {
        self.instruction.result.ty
    }
    /// Returns the authoritative source span.
    #[must_use]
    pub const fn span(self) -> Span {
        self.instruction.result.span
    }
    /// Returns an immutable verified operation view.
    #[must_use]
    pub fn kind(self) -> VerifiedInstructionKind<'program> {
        verified_instruction_kind(self.function, &self.instruction.kind)
    }
}

/// Verified operation view containing only sealed identities.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedInstructionKind<'program> {
    BoolLiteral(bool),
    I32Literal(i32),
    I32Add(ValueIdentity, ValueIdentity),
    I32Sub(ValueIdentity, ValueIdentity),
    I32Mul(ValueIdentity, ValueIdentity),
    I32Neg(ValueIdentity),
    Eq(ValueIdentity, ValueIdentity),
    Ne(ValueIdentity, ValueIdentity),
    I32LtS(ValueIdentity, ValueIdentity),
    I32LeS(ValueIdentity, ValueIdentity),
    I32GtS(ValueIdentity, ValueIdentity),
    I32GeS(ValueIdentity, ValueIdentity),
    DirectCall { callee: FunctionIdentity, arguments: VerifiedValueList<'program> },
}

/// Immutable verified list of value identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedValueList<'program>(&'program [raw::ValueId]);

impl VerifiedValueList<'_> {
    /// Iterates sealed value identities in evaluation order.
    #[must_use]
    pub fn iter(self) -> impl ExactSizeIterator<Item = ValueIdentity> {
        self.0.iter().map(|id| ValueIdentity(id.0))
    }
    /// Returns the number of values.
    #[must_use]
    pub const fn len(self) -> usize {
        self.0.len()
    }
    /// Returns whether the list is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0.is_empty()
    }
}

fn verified_instruction_kind<'program>(
    _function: VerifiedFunction<'program>,
    kind: &'program raw::InstructionKind,
) -> VerifiedInstructionKind<'program> {
    use raw::InstructionKind as R;
    match kind {
        R::BoolLiteral(value) => VerifiedInstructionKind::BoolLiteral(*value),
        R::I32Literal(value) => VerifiedInstructionKind::I32Literal(*value),
        R::I32Add { lhs, rhs } => {
            VerifiedInstructionKind::I32Add(ValueIdentity(lhs.0), ValueIdentity(rhs.0))
        }
        R::I32Sub { lhs, rhs } => {
            VerifiedInstructionKind::I32Sub(ValueIdentity(lhs.0), ValueIdentity(rhs.0))
        }
        R::I32Mul { lhs, rhs } => {
            VerifiedInstructionKind::I32Mul(ValueIdentity(lhs.0), ValueIdentity(rhs.0))
        }
        R::I32Neg { operand } => VerifiedInstructionKind::I32Neg(ValueIdentity(operand.0)),
        R::Eq { lhs, rhs } => {
            VerifiedInstructionKind::Eq(ValueIdentity(lhs.0), ValueIdentity(rhs.0))
        }
        R::Ne { lhs, rhs } => {
            VerifiedInstructionKind::Ne(ValueIdentity(lhs.0), ValueIdentity(rhs.0))
        }
        R::I32LtS { lhs, rhs } => {
            VerifiedInstructionKind::I32LtS(ValueIdentity(lhs.0), ValueIdentity(rhs.0))
        }
        R::I32LeS { lhs, rhs } => {
            VerifiedInstructionKind::I32LeS(ValueIdentity(lhs.0), ValueIdentity(rhs.0))
        }
        R::I32GtS { lhs, rhs } => {
            VerifiedInstructionKind::I32GtS(ValueIdentity(lhs.0), ValueIdentity(rhs.0))
        }
        R::I32GeS { lhs, rhs } => {
            VerifiedInstructionKind::I32GeS(ValueIdentity(lhs.0), ValueIdentity(rhs.0))
        }
        R::DirectCall { callee, arguments } => VerifiedInstructionKind::DirectCall {
            callee: FunctionIdentity {
                module: ModuleIdentity(callee.module.0),
                declaration: callee.declaration,
            },
            arguments: VerifiedValueList(arguments),
        },
    }
}

/// Immutable verified terminator view.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedTerminator<'program> {
    function: VerifiedFunction<'program>,
    terminator: &'program raw::SpannedTerminator,
}

impl<'program> VerifiedTerminator<'program> {
    /// Returns the authoritative terminator span.
    #[must_use]
    pub const fn span(self) -> Span {
        self.terminator.span
    }
    /// Returns the verified terminator operation.
    #[must_use]
    pub fn kind(self) -> VerifiedTerminatorKind<'program> {
        let _ = self.function;
        match &self.terminator.kind {
            raw::Terminator::Return(value) => {
                VerifiedTerminatorKind::Return(ValueIdentity(value.0))
            }
            raw::Terminator::Jump { target, arguments } => VerifiedTerminatorKind::Jump {
                target: BlockIdentity(target.0),
                arguments: VerifiedValueList(arguments),
            },
            raw::Terminator::Branch {
                condition,
                true_target,
                true_arguments,
                false_target,
                false_arguments,
            } => VerifiedTerminatorKind::Branch {
                condition: ValueIdentity(condition.0),
                true_target: BlockIdentity(true_target.0),
                true_arguments: VerifiedValueList(true_arguments),
                false_target: BlockIdentity(false_target.0),
                false_arguments: VerifiedValueList(false_arguments),
            },
        }
    }
}

/// Verified terminator operation containing only sealed identities.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedTerminatorKind<'program> {
    Return(ValueIdentity),
    Jump {
        target: BlockIdentity,
        arguments: VerifiedValueList<'program>,
    },
    Branch {
        condition: ValueIdentity,
        true_target: BlockIdentity,
        true_arguments: VerifiedValueList<'program>,
        false_target: BlockIdentity,
        false_arguments: VerifiedValueList<'program>,
    },
}

#[derive(Clone, Copy)]
struct FunctionKey {
    module: usize,
    function: usize,
}

#[derive(Clone, Copy)]
enum DefinitionLocation {
    Parameter,
    BlockParameter(usize),
    Instruction(usize, usize),
}

#[derive(Clone, Copy)]
struct ValueInfo {
    ty: Type,
    location: DefinitionLocation,
}

#[derive(Clone)]
struct Edge {
    source: usize,
    target: usize,
    arguments: Vec<raw::ValueId>,
}

/// Verifies all `ControlFlowV1` IR claims against the exact source and entry authorities.
///
/// # Errors
///
/// Returns deterministic bounded diagnostics. No verified authority is constructed after any
/// resource, identity, type, graph, call, span, or ABI failure.
pub fn verify(
    program: raw::Program,
    sources: &SourceMap,
    expected_entry: FileId,
) -> Result<VerifiedProgram, Vec<Diagnostic>> {
    let mut errors = Errors::default();
    preflight(&program, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }

    verify_modules(&program, sources, expected_entry, &mut errors);
    if errors.exhausted() {
        return Err(errors.finish());
    }
    let signatures = collect_signatures(&program);
    let mut calls = Vec::<(FunctionKey, FunctionKey)>::new();
    'modules: for (module_index, module) in program.modules.iter().enumerate() {
        for (function_index, function) in module.functions.iter().enumerate() {
            if errors.exhausted() {
                break 'modules;
            }
            verify_function(
                FunctionKey { module: module_index, function: function_index },
                function,
                module.source_file,
                &signatures,
                sources,
                &mut calls,
                &mut errors,
            );
        }
    }
    if errors.exhausted() {
        return Err(errors.finish());
    }
    verify_call_graph(&program, &calls, &mut errors);
    if errors.exhausted() {
        return Err(errors.finish());
    }
    let (abi, abi_indices) = verify_public_abi(&program, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let Some(abi) = abi else {
        return Err(vec![Diagnostic::error(
            "ZRYNA-I2202",
            None,
            "ControlFlowV1 verifier could not construct its bounded scalar ABI table",
            "report this compiler invariant failure with the smallest reproducible source",
        )]);
    };
    Ok(VerifiedProgram { program, abi, abi_indices })
}

#[allow(clippy::too_many_lines)]
fn preflight(program: &raw::Program, errors: &mut Errors) {
    if program.modules.is_empty() {
        errors.push(error(
            "ZRYNA-I2001",
            "ControlFlowV1 program has no entry module",
            "provide the complete nonempty final module closure",
        ));
        return;
    }
    if program.modules.len() > MAX_MODULES {
        errors.limit("module count", MAX_MODULES);
        return;
    }
    let mut functions = 0_usize;
    let mut parameters = 0_usize;
    let mut blocks = 0_usize;
    let mut values = 0_usize;
    let mut edges = 0_usize;
    let mut calls = 0_usize;
    for (module_index, module) in program.modules.iter().enumerate() {
        if module.functions.len() > MAX_FUNCTIONS_PER_MODULE {
            errors.limit_at(
                format!("module #{module_index} function count"),
                MAX_FUNCTIONS_PER_MODULE,
            );
            return;
        }
        if add_total(
            &mut functions,
            module.functions.len(),
            MAX_FUNCTIONS_PER_PROGRAM,
            "function count",
            errors,
        ) {
            return;
        }
        for (function_index, function) in module.functions.iter().enumerate() {
            if function.parameters.len() > MAX_PARAMETERS_PER_FUNCTION {
                errors.limit_at(
                    format!("function #{module_index}:{function_index} parameter count"),
                    MAX_PARAMETERS_PER_FUNCTION,
                );
                return;
            }
            if add_total(
                &mut parameters,
                function.parameters.len(),
                MAX_PARAMETERS_PER_PROGRAM,
                "parameter count",
                errors,
            ) {
                return;
            }
            if function.blocks.is_empty() || function.blocks.len() > MAX_BLOCKS_PER_FUNCTION {
                errors.limit_at(
                    format!("function #{module_index}:{function_index} block count"),
                    MAX_BLOCKS_PER_FUNCTION,
                );
                return;
            }
            if add_total(
                &mut blocks,
                function.blocks.len(),
                MAX_BLOCKS_PER_PROGRAM,
                "block count",
                errors,
            ) {
                return;
            }
            let mut function_values = function.parameters.len();
            let mut function_edges = 0_usize;
            for block in &function.blocks {
                if block.parameters.len() > MAX_BLOCK_PARAMETERS {
                    errors.limit_at("block parameter count".to_owned(), MAX_BLOCK_PARAMETERS);
                    return;
                }
                function_values =
                    checked_sum(function_values, block.parameters.len(), errors, "value count");
                function_values =
                    checked_sum(function_values, block.instructions.len(), errors, "value count");
                if function_values > MAX_VALUES_PER_FUNCTION {
                    errors.limit_at(
                        format!("function #{module_index}:{function_index} value count"),
                        MAX_VALUES_PER_FUNCTION,
                    );
                    return;
                }
                calls = checked_sum(
                    calls,
                    block
                        .instructions
                        .iter()
                        .filter(|instruction| {
                            matches!(instruction.kind, raw::InstructionKind::DirectCall { .. })
                        })
                        .count(),
                    errors,
                    "call edge count",
                );
                if calls > MAX_CALL_EDGES {
                    errors.limit("call edge count", MAX_CALL_EDGES);
                    return;
                }
                for instruction in &block.instructions {
                    if let raw::InstructionKind::DirectCall { arguments, .. } = &instruction.kind
                        && arguments.len() > MAX_PARAMETERS_PER_FUNCTION
                    {
                        errors.limit_at(
                            "direct-call argument count".to_owned(),
                            MAX_PARAMETERS_PER_FUNCTION,
                        );
                        return;
                    }
                }
                if block.terminators.len() == 1 {
                    match &block.terminators[0].kind {
                        raw::Terminator::Return(_) => {}
                        raw::Terminator::Jump { arguments, .. } => {
                            if arguments.len() > MAX_BLOCK_PARAMETERS {
                                errors.limit_at(
                                    "jump argument count".to_owned(),
                                    MAX_BLOCK_PARAMETERS,
                                );
                                return;
                            }
                        }
                        raw::Terminator::Branch { true_arguments, false_arguments, .. } => {
                            if true_arguments.len() > MAX_BLOCK_PARAMETERS {
                                errors.limit_at(
                                    "true branch argument count".to_owned(),
                                    MAX_BLOCK_PARAMETERS,
                                );
                                return;
                            }
                            if false_arguments.len() > MAX_BLOCK_PARAMETERS {
                                errors.limit_at(
                                    "false branch argument count".to_owned(),
                                    MAX_BLOCK_PARAMETERS,
                                );
                                return;
                            }
                        }
                    }
                    function_edges = checked_sum(
                        function_edges,
                        match block.terminators[0].kind {
                            raw::Terminator::Return(_) => 0,
                            raw::Terminator::Jump { .. } => 1,
                            raw::Terminator::Branch { .. } => 2,
                        },
                        errors,
                        "CFG edge count",
                    );
                    if function_edges > MAX_CFG_EDGES_PER_FUNCTION {
                        errors.limit_at(
                            format!("function #{module_index}:{function_index} CFG edge count"),
                            MAX_CFG_EDGES_PER_FUNCTION,
                        );
                        return;
                    }
                }
            }
            if add_total(
                &mut values,
                function_values,
                MAX_VALUES_PER_PROGRAM,
                "value count",
                errors,
            ) {
                return;
            }
            if add_total(
                &mut edges,
                function_edges,
                MAX_CFG_EDGES_PER_PROGRAM,
                "CFG edge count",
                errors,
            ) {
                return;
            }
        }
    }
    if calls > MAX_CALL_EDGES {
        errors.limit("call edge count", MAX_CALL_EDGES);
    }
}

fn checked_sum(current: usize, extra: usize, errors: &mut Errors, label: &str) -> usize {
    current.checked_add(extra).unwrap_or_else(|| {
        errors.limit(label, usize::MAX);
        usize::MAX
    })
}

fn add_total(
    current: &mut usize,
    extra: usize,
    maximum: usize,
    label: &str,
    errors: &mut Errors,
) -> bool {
    *current = checked_sum(*current, extra, errors, label);
    if *current > maximum {
        errors.limit(label, maximum);
        return true;
    }
    false
}

fn verify_modules(
    program: &raw::Program,
    sources: &SourceMap,
    expected_entry: FileId,
    errors: &mut Errors,
) {
    if errors.exhausted() {
        return;
    }
    if program.modules.len() != sources.len() {
        errors.push(error(
            "ZRYNA-I2001",
            "module inventory does not exactly match the final source map",
            "provide every final source module exactly once in normalized path order",
        ));
        if errors.exhausted() {
            return;
        }
    }
    let entry =
        usize::try_from(program.entry_module.0).ok().and_then(|index| program.modules.get(index));
    if entry.map(|module| module.source_file) != Some(expected_entry)
        || sources.source(expected_entry).is_none()
    {
        errors.push(error(
            "ZRYNA-I2002",
            "entry module is not bound to the independently supplied entry source",
            "use the exact entry FileId issued by the final SourceMap",
        ));
        if errors.exhausted() {
            return;
        }
    }
    for (module_index, module) in program.modules.iter().enumerate() {
        let expected_id = u32::try_from(module_index).expect("preflight module bound fits u32");
        let expected_file = sources.verify_file_id(expected_id).ok();
        if module.id != raw::ModuleId(expected_id) || expected_file != Some(module.source_file) {
            errors.push(error(
                "ZRYNA-I2001",
                format!("module #{module_index} has a noncanonical identity or source authority"),
                "order dense modules by final normalized source path",
            ));
            if errors.exhausted() {
                return;
            }
        }
        for (function_index, function) in module.functions.iter().enumerate() {
            let expected_function =
                u32::try_from(function_index).expect("preflight function bound fits u32");
            if function.id.module != module.id || function.id.declaration != expected_function {
                errors.push(error(
                    "ZRYNA-I2003",
                    format!(
                        "function #{module_index}:{function_index} has a noncanonical identity"
                    ),
                    "use its containing module and dense source declaration index",
                ));
                if errors.exhausted() {
                    return;
                }
            }
            if module.id != program.entry_module && function.entry_export.is_some() {
                errors.push(error("ZRYNA-I2004", format!("dependency function #{module_index}:{function_index} claims a public entry export"), "only entry-module exports may enter scalar ABI v1"));
                if errors.exhausted() {
                    return;
                }
            }
        }
    }
}

fn collect_signatures(program: &raw::Program) -> Vec<Vec<(Vec<Type>, Type)>> {
    program
        .modules
        .iter()
        .map(|module| {
            module
                .functions
                .iter()
                .map(|function| {
                    (function.parameters.iter().map(|value| value.ty).collect(), function.result)
                })
                .collect()
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn verify_function(
    key: FunctionKey,
    function: &raw::Function,
    module_file: FileId,
    signatures: &[Vec<(Vec<Type>, Type)>],
    sources: &SourceMap,
    calls: &mut Vec<(FunctionKey, FunctionKey)>,
    errors: &mut Errors,
) {
    if errors.exhausted() {
        return;
    }
    let mut spans_valid = validate_span(function.span, module_file, sources, errors);
    if errors.exhausted() {
        return;
    }
    for parameter in &function.parameters {
        spans_valid &= validate_span(parameter.span, module_file, sources, errors);
        if errors.exhausted() {
            return;
        }
    }
    for block in &function.blocks {
        for parameter in &block.parameters {
            spans_valid &= validate_span(parameter.span, module_file, sources, errors);
            if errors.exhausted() {
                return;
            }
        }
        for instruction in &block.instructions {
            spans_valid &= validate_span(instruction.result.span, module_file, sources, errors);
            if errors.exhausted() {
                return;
            }
        }
        if block.terminators.len() == 1 {
            spans_valid &= validate_span(block.terminators[0].span, module_file, sources, errors);
            if errors.exhausted() {
                return;
            }
        }
    }
    if !spans_valid {
        return;
    }
    validate_type(function.result, None, "function result", errors);
    if errors.exhausted() {
        return;
    }
    let mut values = Vec::<ValueInfo>::new();
    for parameter in &function.parameters {
        define_value(parameter, DefinitionLocation::Parameter, &mut values, errors);
        if errors.exhausted() {
            return;
        }
    }
    for (block_index, block) in function.blocks.iter().enumerate() {
        let expected = u32::try_from(block_index).expect("preflight block bound fits u32");
        if block.id != raw::BlockId(expected) {
            errors.push(error(
                "ZRYNA-I2005",
                format!(
                    "function #{}:{} block #{block_index} has a noncanonical identity",
                    key.module, key.function
                ),
                "use dense block identifiers in arena order",
            ));
            if errors.exhausted() {
                return;
            }
        }
        if block_index == 0 && !block.parameters.is_empty() {
            errors.push(error(
                "ZRYNA-I2006",
                "entry block declares block parameters",
                "function parameters are the only entry-block parameters",
            ));
            if errors.exhausted() {
                return;
            }
        }
        for parameter in &block.parameters {
            define_value(
                parameter,
                DefinitionLocation::BlockParameter(block_index),
                &mut values,
                errors,
            );
            if errors.exhausted() {
                return;
            }
        }
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            define_value(
                &instruction.result,
                DefinitionLocation::Instruction(block_index, instruction_index),
                &mut values,
                errors,
            );
            if errors.exhausted() {
                return;
            }
        }
    }

    let mut edges = Vec::<Edge>::new();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            verify_instruction(
                key,
                block_index,
                instruction_index,
                instruction,
                &values,
                signatures,
                calls,
                errors,
            );
            if errors.exhausted() {
                return;
            }
        }
        if block.terminators.len() != 1 {
            errors.push(error(
                "ZRYNA-I2007",
                format!(
                    "function #{}:{} block #{block_index} has {} terminators; expected exactly one",
                    key.module,
                    key.function,
                    block.terminators.len()
                ),
                "emit exactly one return, jump, or branch terminator",
            ));
            if errors.exhausted() {
                return;
            }
            continue;
        }
        let terminator = &block.terminators[0];
        verify_terminator(key, block_index, function, terminator, &values, &mut edges, errors);
        if errors.exhausted() {
            return;
        }
    }
    verify_cfg(key, function, &values, &edges, errors);
}

fn define_value(
    definition: &raw::ValueDefinition,
    location: DefinitionLocation,
    values: &mut Vec<ValueInfo>,
    errors: &mut Errors,
) {
    if errors.exhausted() {
        return;
    }
    let expected = u32::try_from(values.len()).expect("preflight value bound fits u32");
    if definition.id != raw::ValueId(expected) {
        errors.push(error("ZRYNA-I2008", format!("value definition claims #{}, expected dense value #{expected}", definition.id.0), "allocate function parameters, then block parameters and instruction results in canonical order"));
        if errors.exhausted() {
            return;
        }
    }
    validate_type(definition.ty, Some(definition.span), "value", errors);
    if errors.exhausted() {
        return;
    }
    values.push(ValueInfo { ty: definition.ty, location });
}

fn validate_type(ty: Type, span: Option<Span>, label: &str, errors: &mut Errors) {
    if errors.exhausted() {
        return;
    }
    if ty == Type::Unit {
        errors.push(error_at(
            "ZRYNA-I2009",
            span,
            format!("{label} uses unsupported unit type"),
            "ControlFlowV1 admits only bool and i32 values",
        ));
    }
}

fn validate_span(
    span: Span,
    module_file: FileId,
    sources: &SourceMap,
    errors: &mut Errors,
) -> bool {
    if errors.exhausted() {
        return false;
    }
    match sources.resolve(span) {
        Ok(resolved) if resolved.source().id() == module_file => true,
        Ok(_) => {
            errors.push(error(
                "ZRYNA-I2023",
                "IR span belongs to a different module source file",
                "bind every function, value, instruction, and terminator span to its containing module",
            ));
            false
        }
        Err(source_error) => {
            errors.push(Diagnostic::from_source_error(&source_error));
            false
        }
    }
}

fn value_info(id: raw::ValueId, values: &[ValueInfo]) -> Option<(usize, ValueInfo)> {
    usize::try_from(id.0)
        .ok()
        .and_then(|index| values.get(index).copied().map(|value| (index, value)))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn verify_instruction(
    key: FunctionKey,
    block: usize,
    position: usize,
    instruction: &raw::Instruction,
    values: &[ValueInfo],
    signatures: &[Vec<(Vec<Type>, Type)>],
    calls: &mut Vec<(FunctionKey, FunctionKey)>,
    errors: &mut Errors,
) {
    use raw::InstructionKind as I;

    if errors.exhausted() {
        return;
    }
    let result = instruction.result.ty;
    match &instruction.kind {
        I::BoolLiteral(_) => {
            expect_result(result, Type::Bool, instruction.result.span, "bool literal", errors);
        }
        I::I32Literal(_) => {
            expect_result(result, Type::I32, instruction.result.span, "i32 literal", errors);
        }
        I::I32Neg { operand } => {
            expect_values(
                &[*operand],
                &[Type::I32],
                result,
                Type::I32,
                block,
                position,
                values,
                instruction.result.span,
                errors,
            );
        }
        I::I32Add { lhs, rhs } | I::I32Sub { lhs, rhs } | I::I32Mul { lhs, rhs } => {
            expect_values(
                &[*lhs, *rhs],
                &[Type::I32, Type::I32],
                result,
                Type::I32,
                block,
                position,
                values,
                instruction.result.span,
                errors,
            );
        }
        I::I32LtS { lhs, rhs }
        | I::I32LeS { lhs, rhs }
        | I::I32GtS { lhs, rhs }
        | I::I32GeS { lhs, rhs } => {
            expect_values(
                &[*lhs, *rhs],
                &[Type::I32, Type::I32],
                result,
                Type::Bool,
                block,
                position,
                values,
                instruction.result.span,
                errors,
            );
        }
        I::Eq { lhs, rhs } | I::Ne { lhs, rhs } => {
            let left = checked_use(*lhs, block, position, values, instruction.result.span, errors);
            if errors.exhausted() {
                return;
            }
            let right = checked_use(*rhs, block, position, values, instruction.result.span, errors);
            if errors.exhausted() {
                return;
            }
            if result != Type::Bool
                || left.map(|(_, value)| value.ty) != right.map(|(_, value)| value.ty)
            {
                errors.push(error_at(
                    "ZRYNA-I2010",
                    Some(instruction.result.span),
                    "equality requires two values of one exact scalar type and a bool result",
                    "use matching bool or matching i32 operands",
                ));
            }
        }
        I::DirectCall { callee, arguments } => {
            let target = usize::try_from(callee.module.0).ok().and_then(|module| {
                usize::try_from(callee.declaration)
                    .ok()
                    .map(|function| FunctionKey { module, function })
            });
            let signature = target.and_then(|target| {
                signatures
                    .get(target.module)
                    .and_then(|module| module.get(target.function))
                    .map(|signature| (target, signature))
            });
            let Some((target, (parameters, target_result))) = signature else {
                errors.push(error_at(
                    "ZRYNA-I2011",
                    Some(instruction.result.span),
                    "direct call targets an unknown function identity",
                    "call one function in the sealed module closure",
                ));
                return;
            };
            if errors.exhausted() {
                return;
            }
            calls.push((key, target));
            if arguments.len() != parameters.len() || result != *target_result {
                errors.push(error_at(
                    "ZRYNA-I2011",
                    Some(instruction.result.span),
                    "direct call signature does not match its target",
                    "use exact arity, argument types, and result type",
                ));
                if errors.exhausted() {
                    return;
                }
            }
            for (argument_index, argument) in arguments.iter().enumerate() {
                let actual = checked_use(
                    *argument,
                    block,
                    position,
                    values,
                    instruction.result.span,
                    errors,
                );
                if errors.exhausted() {
                    return;
                }
                if actual.map(|(_, value)| value.ty) != parameters.get(argument_index).copied() {
                    errors.push(error_at(
                        "ZRYNA-I2011",
                        Some(instruction.result.span),
                        format!("direct call argument #{argument_index} has the wrong type"),
                        "preserve source argument order and exact parameter types",
                    ));
                    if errors.exhausted() {
                        return;
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn expect_values(
    operands: &[raw::ValueId],
    expected: &[Type],
    result: Type,
    expected_result: Type,
    block: usize,
    position: usize,
    values: &[ValueInfo],
    span: Span,
    errors: &mut Errors,
) {
    if errors.exhausted() {
        return;
    }
    let valid = operands.iter().zip(expected).all(|(operand, expected)| {
        checked_use(*operand, block, position, values, span, errors)
            .is_some_and(|(_, value)| value.ty == *expected)
    });
    if errors.exhausted() {
        return;
    }
    if !valid || result != expected_result {
        errors.push(error_at(
            "ZRYNA-I2010",
            Some(span),
            "instruction operand or result type is invalid",
            "use exact ControlFlowV1 operation types",
        ));
    }
}

fn expect_result(actual: Type, expected: Type, span: Span, label: &str, errors: &mut Errors) {
    if errors.exhausted() {
        return;
    }
    if actual != expected {
        errors.push(error_at(
            "ZRYNA-I2010",
            Some(span),
            format!("{label} has the wrong result type"),
            "use the exact literal type",
        ));
    }
}

fn checked_use(
    id: raw::ValueId,
    block: usize,
    position: usize,
    values: &[ValueInfo],
    span: Span,
    errors: &mut Errors,
) -> Option<(usize, ValueInfo)> {
    if errors.exhausted() {
        return None;
    }
    let Some((index, value)) = value_info(id, values) else {
        errors.push(error_at(
            "ZRYNA-I2012",
            Some(span),
            format!("operand references unknown value #{}", id.0),
            "reference one value in the same verified function",
        ));
        return None;
    };
    if let DefinitionLocation::Instruction(definition_block, definition_position) = value.location
        && definition_block == block
        && definition_position >= position
    {
        errors.push(error_at(
            "ZRYNA-I2013",
            Some(span),
            format!("value #{index} is used before its definition"),
            "use only earlier instruction results in the same block",
        ));
    }
    Some((index, value))
}

fn verify_terminator(
    key: FunctionKey,
    block: usize,
    function: &raw::Function,
    terminator: &raw::SpannedTerminator,
    values: &[ValueInfo],
    edges: &mut Vec<Edge>,
    errors: &mut Errors,
) {
    if errors.exhausted() {
        return;
    }
    let position = function.blocks[block].instructions.len();
    match &terminator.kind {
        raw::Terminator::Return(value) => {
            let actual = checked_use(*value, block, position, values, terminator.span, errors)
                .map(|(_, value)| value.ty);
            if errors.exhausted() {
                return;
            }
            if actual != Some(function.result) {
                errors.push(error_at(
                    "ZRYNA-I2014",
                    Some(terminator.span),
                    "return value has the wrong type",
                    "return one value of the exact function result type",
                ));
            }
        }
        raw::Terminator::Jump { target, arguments } => {
            verify_edge(
                key,
                block,
                *target,
                arguments,
                function,
                values,
                terminator.span,
                edges,
                errors,
            );
        }
        raw::Terminator::Branch {
            condition,
            true_target,
            true_arguments,
            false_target,
            false_arguments,
        } => {
            if checked_use(*condition, block, position, values, terminator.span, errors)
                .map(|(_, value)| value.ty)
                != Some(Type::Bool)
            {
                errors.push(error_at(
                    "ZRYNA-I2015",
                    Some(terminator.span),
                    "branch condition is not bool",
                    "branch only on an exact bool value",
                ));
            }
            if errors.exhausted() {
                return;
            }
            verify_edge(
                key,
                block,
                *true_target,
                true_arguments,
                function,
                values,
                terminator.span,
                edges,
                errors,
            );
            if errors.exhausted() {
                return;
            }
            verify_edge(
                key,
                block,
                *false_target,
                false_arguments,
                function,
                values,
                terminator.span,
                edges,
                errors,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn verify_edge(
    key: FunctionKey,
    source: usize,
    target: raw::BlockId,
    arguments: &[raw::ValueId],
    function: &raw::Function,
    values: &[ValueInfo],
    span: Span,
    edges: &mut Vec<Edge>,
    errors: &mut Errors,
) {
    if errors.exhausted() {
        return;
    }
    let Some(target_index) =
        usize::try_from(target.0).ok().filter(|index| *index < function.blocks.len())
    else {
        errors.push(error_at(
            "ZRYNA-I2016",
            Some(span),
            format!(
                "function #{}:{} has an edge to unknown block #{}",
                key.module, key.function, target.0
            ),
            "target one block in the same function",
        ));
        return;
    };
    if target_index == 0 {
        errors.push(error_at(
            "ZRYNA-I2016",
            Some(span),
            "control-flow edge targets the entry block",
            "entry block zero must have no predecessor",
        ));
        if errors.exhausted() {
            return;
        }
    }
    let parameters = &function.blocks[target_index].parameters;
    if arguments.len() != parameters.len() {
        errors.push(error_at(
            "ZRYNA-I2017",
            Some(span),
            "block argument arity does not match target parameters",
            "pass one exact argument for each target block parameter",
        ));
        if errors.exhausted() {
            return;
        }
    }
    let position = function.blocks[source].instructions.len();
    for (index, argument) in arguments.iter().enumerate() {
        let actual = checked_use(*argument, source, position, values, span, errors)
            .map(|(_, value)| value.ty);
        if errors.exhausted() {
            return;
        }
        if actual != parameters.get(index).map(|parameter| parameter.ty) {
            errors.push(error_at(
                "ZRYNA-I2017",
                Some(span),
                format!("block argument #{index} has the wrong type"),
                "match target block parameter types exactly",
            ));
            if errors.exhausted() {
                return;
            }
        }
    }
    if errors.exhausted() {
        return;
    }
    edges.push(Edge { source, target: target_index, arguments: arguments.to_vec() });
}

#[allow(clippy::too_many_lines)]
fn verify_cfg(
    key: FunctionKey,
    function: &raw::Function,
    values: &[ValueInfo],
    edges: &[Edge],
    errors: &mut Errors,
) {
    if errors.exhausted() || function.blocks.is_empty() {
        return;
    }
    let count = function.blocks.len();
    let mut successors = vec![Vec::<usize>::new(); count];
    let mut predecessors = vec![Vec::<usize>::new(); count];
    for edge in edges {
        successors[edge.source].push(edge.target);
        predecessors[edge.target].push(edge.source);
        let _ = edge.arguments.len();
    }
    for list in &mut successors {
        list.sort_unstable();
    }
    for list in &mut predecessors {
        list.sort_unstable();
    }
    let reachable = reachable_from_entry(&successors);
    for block in 1..count {
        if predecessors[block].is_empty() || !reachable[block] {
            errors.push(error(
                "ZRYNA-I2018",
                format!(
                    "function #{}:{} block #{block} is unreachable or has no predecessor",
                    key.module, key.function
                ),
                "emit only blocks reachable from entry",
            ));
            if errors.exhausted() {
                return;
            }
        }
    }
    if reachable.iter().any(|reachable| !reachable) {
        return;
    }
    // This fallback is not an independently triggerable raw-IR category: after the reachability
    // proof above, the bounded Cooper iteration must assign every immediate dominator. Retaining a
    // distinct diagnostic fails closed if that internal construction invariant ever regresses.
    let Some(idom) = immediate_dominators(&successors, &predecessors) else {
        errors.push(error(
            "ZRYNA-I2019",
            "failed to construct deterministic dominators",
            "provide one reachable reducible CFG",
        ));
        return;
    };
    let (entry_time, exit_time) = dominator_intervals(&idom);
    let dominates = |left: usize, right: usize| {
        entry_time[left] <= entry_time[right] && exit_time[right] <= exit_time[left]
    };

    for (use_block, block) in function.blocks.iter().enumerate() {
        for (position, instruction) in block.instructions.iter().enumerate() {
            for operand in instruction_operands(&instruction.kind) {
                verify_dominance(
                    operand,
                    use_block,
                    position,
                    values,
                    &dominates,
                    instruction.result.span,
                    errors,
                );
                if errors.exhausted() {
                    return;
                }
            }
        }
        if block.terminators.len() == 1 {
            let position = block.instructions.len();
            for operand in terminator_operands(&block.terminators[0].kind) {
                verify_dominance(
                    operand,
                    use_block,
                    position,
                    values,
                    &dominates,
                    block.terminators[0].span,
                    errors,
                );
                if errors.exhausted() {
                    return;
                }
            }
        }
    }

    let mut forward_indegree = vec![0_usize; count];
    let mut forward = vec![Vec::<usize>::new(); count];
    let mut loops = BTreeMap::<usize, Vec<bool>>::new();
    for edge in edges {
        if dominates(edge.target, edge.source) {
            let members = loops.entry(edge.target).or_insert_with(|| vec![false; count]);
            members[edge.target] = true;
            let mut stack = vec![edge.source];
            while let Some(node) = stack.pop() {
                if members[node] {
                    continue;
                }
                members[node] = true;
                for predecessor in &predecessors[node] {
                    if *predecessor != edge.target {
                        stack.push(*predecessor);
                    }
                }
            }
        } else {
            forward[edge.source].push(edge.target);
            forward_indegree[edge.target] = forward_indegree[edge.target].saturating_add(1);
        }
    }
    let mut queue = VecDeque::new();
    for (index, degree) in forward_indegree.iter().enumerate() {
        if *degree == 0 {
            queue.push_back(index);
        }
    }
    let mut seen = 0_usize;
    while let Some(node) = queue.pop_front() {
        seen += 1;
        for target in &forward[node] {
            forward_indegree[*target] -= 1;
            if forward_indegree[*target] == 0 {
                queue.push_back(*target);
            }
        }
    }
    if seen != count {
        errors.push(error(
            "ZRYNA-I2020",
            format!("function #{}:{} contains irreducible control flow", key.module, key.function),
            "every cycle must have one dominating nonentry header",
        ));
        if errors.exhausted() {
            return;
        }
    }
    let maximum_nesting = (0..count)
        .map(|block| loops.values().filter(|members| members[block]).count())
        .max()
        .unwrap_or(0);
    if maximum_nesting > MAX_LOOP_NESTING {
        errors.limit_at(
            format!("function #{}:{} loop nesting", key.module, key.function),
            MAX_LOOP_NESTING,
        );
    }
}

fn reachable_from_entry(successors: &[Vec<usize>]) -> Vec<bool> {
    let mut reachable = vec![false; successors.len()];
    let mut stack = vec![0_usize];
    while let Some(block) = stack.pop() {
        if reachable[block] {
            continue;
        }
        reachable[block] = true;
        for target in successors[block].iter().rev() {
            stack.push(*target);
        }
    }
    reachable
}

pub(crate) fn immediate_dominators(
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
) -> Option<Vec<usize>> {
    let count = successors.len();
    let mut visited = vec![false; count];
    let mut order = Vec::with_capacity(count);
    let mut stack = vec![(0_usize, 0_usize)];
    visited[0] = true;
    while let Some((node, child)) = stack.pop() {
        if child < successors[node].len() {
            stack.push((node, child + 1));
            let next = successors[node][child];
            if !visited[next] {
                visited[next] = true;
                stack.push((next, 0));
            }
        } else {
            order.push(node);
        }
    }
    if order.len() != count {
        return None;
    }
    order.reverse();
    let mut position = vec![0_usize; count];
    for (index, block) in order.iter().enumerate() {
        position[*block] = index;
    }
    let mut idom = vec![usize::MAX; count];
    idom[0] = 0;
    for _ in 0..count {
        let mut changed = false;
        for block in order.iter().copied().skip(1) {
            let mut defined =
                predecessors[block].iter().copied().filter(|pred| idom[*pred] != usize::MAX);
            let Some(mut candidate) = defined.next() else {
                continue;
            };
            for predecessor in defined {
                candidate = intersect(candidate, predecessor, &idom, &position);
            }
            if idom[block] != candidate {
                idom[block] = candidate;
                changed = true;
            }
        }
        if !changed {
            return idom.iter().all(|value| *value != usize::MAX).then_some(idom);
        }
    }
    None
}

fn intersect(mut left: usize, mut right: usize, idom: &[usize], position: &[usize]) -> usize {
    while left != right {
        while position[left] > position[right] {
            left = idom[left];
        }
        while position[right] > position[left] {
            right = idom[right];
        }
    }
    left
}

pub(crate) fn dominator_intervals(idom: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let mut children = vec![Vec::<usize>::new(); idom.len()];
    for block in 1..idom.len() {
        children[idom[block]].push(block);
    }
    let mut entry = vec![0_usize; idom.len()];
    let mut exit = vec![0_usize; idom.len()];
    let mut clock = 0_usize;
    let mut stack = vec![(0_usize, false)];
    while let Some((node, exiting)) = stack.pop() {
        if exiting {
            exit[node] = clock;
            clock += 1;
            continue;
        }
        entry[node] = clock;
        clock += 1;
        stack.push((node, true));
        for child in children[node].iter().rev() {
            stack.push((*child, false));
        }
    }
    (entry, exit)
}

fn verify_dominance(
    operand: raw::ValueId,
    use_block: usize,
    use_position: usize,
    values: &[ValueInfo],
    dominates: &impl Fn(usize, usize) -> bool,
    span: Span,
    errors: &mut Errors,
) {
    if errors.exhausted() {
        return;
    }
    let Some((_, value)) = value_info(operand, values) else {
        return;
    };
    let valid = match value.location {
        DefinitionLocation::Parameter => true,
        DefinitionLocation::BlockParameter(block) => dominates(block, use_block),
        DefinitionLocation::Instruction(block, position) => {
            if block == use_block {
                position < use_position
            } else {
                dominates(block, use_block)
            }
        }
    };
    if !valid {
        errors.push(error_at(
            "ZRYNA-I2013",
            Some(span),
            format!("value #{} does not dominate its use", operand.0),
            "use only values available on every path to this operation",
        ));
    }
}

fn instruction_operands(kind: &raw::InstructionKind) -> Vec<raw::ValueId> {
    use raw::InstructionKind as I;
    match kind {
        I::BoolLiteral(_) | I::I32Literal(_) => Vec::new(),
        I::I32Neg { operand } => vec![*operand],
        I::I32Add { lhs, rhs }
        | I::I32Sub { lhs, rhs }
        | I::I32Mul { lhs, rhs }
        | I::Eq { lhs, rhs }
        | I::Ne { lhs, rhs }
        | I::I32LtS { lhs, rhs }
        | I::I32LeS { lhs, rhs }
        | I::I32GtS { lhs, rhs }
        | I::I32GeS { lhs, rhs } => vec![*lhs, *rhs],
        I::DirectCall { arguments, .. } => arguments.clone(),
    }
}

fn terminator_operands(kind: &raw::Terminator) -> Vec<raw::ValueId> {
    match kind {
        raw::Terminator::Return(value) => vec![*value],
        raw::Terminator::Jump { arguments, .. } => arguments.clone(),
        raw::Terminator::Branch { condition, true_arguments, false_arguments, .. } => {
            let mut values = Vec::with_capacity(1 + true_arguments.len() + false_arguments.len());
            values.push(*condition);
            values.extend(true_arguments);
            values.extend(false_arguments);
            values
        }
    }
}

fn verify_call_graph(
    program: &raw::Program,
    calls: &[(FunctionKey, FunctionKey)],
    errors: &mut Errors,
) {
    if errors.exhausted() {
        return;
    }
    let offsets = function_offsets(program);
    let function_count = offsets.last().copied().unwrap_or(0);
    let mut successors = vec![Vec::<usize>::new(); function_count];
    let mut indegree = vec![0_usize; function_count];
    for (caller, callee) in calls {
        let source = offsets[caller.module] + caller.function;
        let target = offsets[callee.module] + callee.function;
        successors[source].push(target);
        indegree[target] = indegree[target].saturating_add(1);
    }
    let mut queue = VecDeque::new();
    for (index, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            queue.push_back(index);
        }
    }
    let mut depth = vec![1_usize; function_count];
    let mut seen = 0_usize;
    while let Some(function) = queue.pop_front() {
        seen += 1;
        for callee in &successors[function] {
            depth[*callee] = depth[*callee].max(depth[function].saturating_add(1));
            indegree[*callee] -= 1;
            if indegree[*callee] == 0 {
                queue.push_back(*callee);
            }
        }
    }
    if seen != function_count {
        errors.push(error(
            "ZRYNA-I2021",
            "resolved direct-call graph contains a cycle",
            "ControlFlowV1 requires an acyclic complete call graph",
        ));
    } else if depth.into_iter().max().unwrap_or(0) > MAX_STATIC_CALL_DEPTH {
        errors.limit("static call depth", MAX_STATIC_CALL_DEPTH);
    }
}

fn function_offsets(program: &raw::Program) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(program.modules.len() + 1);
    let mut total = 0_usize;
    for module in &program.modules {
        offsets.push(total);
        total += module.functions.len();
    }
    offsets.push(total);
    offsets
}

fn verify_public_abi(
    program: &raw::Program,
    errors: &mut Errors,
) -> (Option<zryna_abi::VerifiedScalarAbiModule>, Vec<Vec<Option<usize>>>) {
    if errors.exhausted() {
        return (None, Vec::new());
    }
    let mut exports = Vec::new();
    let mut indices =
        program.modules.iter().map(|module| vec![None; module.functions.len()]).collect::<Vec<_>>();
    let entry = usize::try_from(program.entry_module.0).ok();
    if let Some(entry) = entry.and_then(|index| program.modules.get(index).map(|_| index)) {
        for (function_index, function) in program.modules[entry].functions.iter().enumerate() {
            let Some(name) = &function.entry_export else {
                continue;
            };
            let abi_index = exports.len();
            indices[entry][function_index] = Some(abi_index);
            exports.push(raw_abi::Export::new(
                name.clone(),
                raw_abi::Signature::new(
                    function
                        .parameters
                        .iter()
                        .map(|parameter| raw_abi_type(parameter.ty))
                        .collect(),
                    raw_abi_type(function.result),
                ),
            ));
        }
    }
    match verify_v1(raw_abi::Module::new(exports)) {
        Ok(abi) => (Some(abi), indices),
        Err(violations) => {
            for violation in violations {
                let (message, guidance) = match violation.kind() {
                    AbiViolationKind::InvalidLogicalName => (
                        "entry export has an invalid logical name",
                        "use the exact scalar ABI v1 logical-name grammar",
                    ),
                    AbiViolationKind::DuplicateLogicalName { .. } => (
                        "entry exports contain a duplicate logical name",
                        "give every public entry export a unique exact name",
                    ),
                    AbiViolationKind::PortableNameCollision { .. } => (
                        "entry exports collide under portable target identity",
                        "use names unique under ASCII case folding",
                    ),
                    AbiViolationKind::UnsupportedScalarType => (
                        "entry export uses an unsupported scalar ABI type",
                        "use only bool and i32",
                    ),
                    AbiViolationKind::TooManyExports
                    | AbiViolationKind::TooManyParameters
                    | AbiViolationKind::TooManyParametersInModule => (
                        "entry scalar ABI exceeds its resource limits",
                        "reduce the public entry surface",
                    ),
                    AbiViolationKind::ViolationBudgetExceeded => (
                        "entry scalar ABI diagnostics exceeded their limit",
                        "fix earlier ABI diagnostics",
                    ),
                };
                errors.push(error("ZRYNA-I2022", message, guidance));
                if errors.exhausted() {
                    break;
                }
            }
            (None, indices)
        }
    }
}

const fn raw_abi_type(ty: Type) -> raw_abi::Type {
    match ty {
        Type::Unit => raw_abi::Type::Unit,
        Type::Bool => raw_abi::Type::Bool,
        Type::I32 => raw_abi::Type::I32,
    }
}

fn error(code: &'static str, message: impl Into<String>, guidance: &'static str) -> Diagnostic {
    Diagnostic::error(code, None, message, guidance)
}

fn error_at(
    code: &'static str,
    span: Option<Span>,
    message: impl Into<String>,
    guidance: &'static str,
) -> Diagnostic {
    match span {
        Some(span) => Diagnostic::error_at(code, span, message, guidance),
        None => error(code, message, guidance),
    }
}

#[derive(Default)]
struct Errors {
    diagnostics: Vec<Diagnostic>,
    exhausted: bool,
}

impl Errors {
    fn push(&mut self, diagnostic: Diagnostic) {
        if self.exhausted {
            return;
        }
        if self.diagnostics.len() < MAX_DIAGNOSTICS - 1 {
            self.diagnostics.push(diagnostic);
            return;
        }
        self.diagnostics.push(error(
            "ZRYNA-I2202",
            format!("ControlFlowV1 verification reached its diagnostic limit of {MAX_DIAGNOSTICS}"),
            "fix retained diagnostics before verifying again",
        ));
        self.exhausted = true;
    }
    fn limit(&mut self, label: &str, maximum: usize) {
        self.limit_at(label.to_owned(), maximum);
    }
    #[allow(clippy::needless_pass_by_value)]
    fn limit_at(&mut self, label: String, maximum: usize) {
        if self.exhausted {
            return;
        }
        self.diagnostics.push(error(
            "ZRYNA-I2201",
            format!("ControlFlowV1 {label} exceeds its limit of {maximum}"),
            "reduce the program before IR verification",
        ));
        self.exhausted = true;
    }
    fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
    const fn exhausted(&self) -> bool {
        self.exhausted
    }
    fn finish(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

#[cfg(test)]
mod tests;
