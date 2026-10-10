//! Verified data and ownership IR for the `DataOwnershipV1` profile.
//!
//! This module is isolated from the stable M1 and M2 verifier surfaces. Values in
//! [`crate::data_ownership_v1::raw`] are
//! untrusted claims. Only [`verify`] can bind them to the exact source, layout, scalar-ABI, CFG,
//! ownership, and cleanup authorities exposed through opaque immutable views.
use std::collections::{BTreeSet, VecDeque};
use zryna_abi::{raw as raw_abi, verify_v1};
use zryna_diagnostics::Diagnostic;
use zryna_layout::{
    StorageTarget, TypeCategory, TypeId as LayoutTypeId, TypeUniverseIdentity, VerifiedLayouts,
};
use zryna_source::{FileId, SourceMap, SourceMapIdentity, Span};
mod backend_view;
mod cleanup_references;
mod generic_clone;
mod generic_static_places;
mod handle_aware_clone;
mod transient_edges;
mod weak_upgrade_shape;
pub use backend_view::*;
use cleanup_references::{cleanup_references, instruction_cleanup};
mod instruction_operands;
use instruction_operands::{consuming_instruction_operands, instruction_operands};
mod state_replay;
pub use generic_clone::{
    VerifiedGenericClone, VerifiedGenericCloneFrontier, VerifiedGenericCloneSource,
};
pub use handle_aware_clone::{
    VerifiedHandleAwareClone, VerifiedHandleAwareCloneFrontier, VerifiedHandleAwareCloneSource,
    VerifiedHandleAwareCloneSourceAuthority, VerifiedHandleCloneRecipeKind,
    VerifiedHandleCloneRecipeNode,
};
use state_replay::derive_state_before;
pub use transient_edges::{
    MAX_CONTINUED_INDEXED_IDENTITIES_PER_FUNCTION, VerifiedContinuedIndexedAccess,
};
pub use weak_upgrade_shape::WeakUpgradeShape;

/// Maximum modules in one program.
pub const MAX_MODULES: usize = 4_096;
/// Maximum functions in one module.
pub const MAX_FUNCTIONS_PER_MODULE: usize = 4_096;
/// Maximum functions in one program.
pub const MAX_FUNCTIONS_PER_PROGRAM: usize = 16_384;
/// Maximum value parameters in one function.
pub const MAX_PARAMETERS_PER_FUNCTION: usize = 256;
/// Maximum value parameters in one program.
pub const MAX_PARAMETERS_PER_PROGRAM: usize = 262_144;
/// Maximum blocks in one function.
pub const MAX_BLOCKS_PER_FUNCTION: usize = 4_096;
/// Maximum blocks in one program.
pub const MAX_BLOCKS_PER_PROGRAM: usize = 65_536;
/// Maximum parameters on one block.
pub const MAX_BLOCK_PARAMETERS: usize = 256;
/// Maximum values in one function.
pub const MAX_VALUES_PER_FUNCTION: usize = 16_384;
/// Maximum values in one program.
pub const MAX_VALUES_PER_PROGRAM: usize = 262_144;
/// Maximum CFG edges in one function.
pub const MAX_CFG_EDGES_PER_FUNCTION: usize = 8_192;
/// Maximum CFG edges in one program.
pub const MAX_CFG_EDGES_PER_PROGRAM: usize = 131_072;
/// Maximum direct-call edges in one program.
pub const MAX_CALL_EDGES: usize = 65_536;
/// Maximum acyclic direct-call depth.
pub const MAX_STATIC_CALL_DEPTH: usize = 128;
/// Maximum verified loop nesting.
pub const MAX_LOOP_NESTING: usize = 128;
/// Maximum nominal declarations in one program.
pub const MAX_NOMINAL_DECLARATIONS: usize = 4_096;
/// Maximum types in one sealed universe.
pub const MAX_INSTANTIATED_TYPES: usize = 65_536;
/// Maximum aggregate construction operands in one program.
pub const MAX_AGGREGATE_OPERANDS: usize = 262_144;
/// Maximum ownership places in one function.
pub const MAX_PLACES_PER_FUNCTION: usize = 65_536;
/// Maximum ownership transitions in one function.
pub const MAX_OWNERSHIP_TRANSITIONS_PER_FUNCTION: usize = 262_144;
/// Maximum simultaneously active borrows in one function.
pub const MAX_ACTIVE_BORROWS_PER_FUNCTION: usize = 16_384;
/// Maximum derived drop actions in one function.
pub const MAX_DROP_ACTIONS_PER_FUNCTION: usize = 262_144;
/// Maximum cleanup plans, including empty plans, in one function.
pub const MAX_CLEANUP_PLANS_PER_FUNCTION: usize = 65_536;
/// Maximum cumulative UTF-8 literal bytes retained by one program.
pub const MAX_STRING_LITERAL_BYTES: usize = 8 * 1024 * 1024;
/// Maximum retained diagnostics including the terminal diagnostic.
pub const MAX_DIAGNOSTICS: usize = 256;

/// Exact non-executable runtime contract identity retained by this IR boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub enum RuntimeContractIdentity {
    /// The frozen ownership runtime ABI v1 contract. Issue #80 supplies a separate sealed,
    /// non-executable declaration authority.
    OwnershipRuntimeV1,
    /// Command H1 v1, including its separate language/canonical memory partition.
    CommandH1V1,
}

/// Untrusted `DataOwnershipV1` claims produced by future semantic lowering.
#[allow(missing_docs)]
pub mod raw {
    use serde::Serialize;
    use zryna_source::{FileId, Span};

    use super::RuntimeContractIdentity;

    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct ModuleId(pub u32);
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct FunctionId {
        pub module: ModuleId,
        pub declaration: u32,
    }
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct BlockId(pub u32);
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct ValueId(pub u32);
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct PlaceId(pub u32);
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct BorrowId(pub u32);
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct CleanupPlanId(pub u32);
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
    pub struct TypeId(pub u32);

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
    pub struct AuthorityClaims {
        pub runtime: RuntimeContractIdentity,
        pub type_universe: [u8; 32],
        pub linear32_fingerprint: [u8; 32],
        pub linux_x86_64_fingerprint: [u8; 32],
    }

    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct Program {
        pub authorities: AuthorityClaims,
        pub entry_module: ModuleId,
        pub modules: Vec<Module>,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct Module {
        pub id: ModuleId,
        pub source_file: FileId,
        pub data_declarations: u32,
        pub functions: Vec<Function>,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct Function {
        pub id: FunctionId,
        pub entry_export: Option<String>,
        pub span: Span,
        pub parameters: Vec<ValueDefinition>,
        pub borrow_parameters: Vec<BorrowParameter>,
        pub result: TypeId,
        pub places: Vec<Place>,
        pub blocks: Vec<Block>,
        pub cleanup_plans: Vec<CleanupPlan>,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
    pub struct ValueDefinition {
        pub id: ValueId,
        pub ty: TypeId,
        pub span: Span,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
    pub struct BorrowParameter {
        pub id: BorrowId,
        pub referent: TypeId,
        pub access: BorrowAccess,
        pub span: Span,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
    pub enum BorrowAccess {
        Shared,
        Exclusive,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct Place {
        pub id: PlaceId,
        pub ty: TypeId,
        pub span: Span,
        pub kind: PlaceKind,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub enum PlaceKind {
        Parameter(u32),
        Local(u32),
        Temporary(ValueId),
        StructField { base: PlaceId, ordinal: u32 },
        EnumPayload { base: PlaceId, variant: u32 },
        FixedArrayConstant { base: PlaceId, index: u32 },
    }

    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct BorrowDefinition {
        pub id: BorrowId,
        pub place: PlaceId,
        pub access: BorrowAccess,
        pub span: Span,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct Instruction {
        pub result: Option<ValueDefinition>,
        pub span: Span,
        pub kind: InstructionKind,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub enum CallArgument {
        Value(ValueId),
        Borrow(BorrowId),
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub enum InstructionKind {
        BoolLiteral(bool),
        I32Literal(i32),
        I32Add {
            lhs: ValueId,
            rhs: ValueId,
        },
        I32Sub {
            lhs: ValueId,
            rhs: ValueId,
        },
        I32Mul {
            lhs: ValueId,
            rhs: ValueId,
        },
        I32Neg {
            operand: ValueId,
        },
        Eq {
            lhs: ValueId,
            rhs: ValueId,
        },
        Ne {
            lhs: ValueId,
            rhs: ValueId,
        },
        I32LtS {
            lhs: ValueId,
            rhs: ValueId,
        },
        I32LeS {
            lhs: ValueId,
            rhs: ValueId,
        },
        I32GtS {
            lhs: ValueId,
            rhs: ValueId,
        },
        I32GeS {
            lhs: ValueId,
            rhs: ValueId,
        },
        DirectCall {
            callee: FunctionId,
            arguments: Vec<CallArgument>,
            cleanup: CleanupPlanId,
        },
        StructConstruct {
            fields: Vec<ValueId>,
            cleanup: Option<CleanupPlanId>,
        },
        EnumConstruct {
            variant: u32,
            payload: Option<ValueId>,
            cleanup: Option<CleanupPlanId>,
        },
        EnvironmentLookup {
            key: String,
            cleanup: CleanupPlanId,
        },
        FixedArrayConstruct {
            elements: Vec<ValueId>,
            cleanup: Option<CleanupPlanId>,
        },
        CopyFromPlace {
            place: PlaceId,
        },
        MoveFromPlace {
            place: PlaceId,
        },
        GenericMoveFromPlace {
            place: PlaceId,
        },
        ClonePlace {
            place: PlaceId,
            cleanup: CleanupPlanId,
            element_cleanup: Option<CleanupPlanId>,
        },
        GenericClonePlace {
            place: PlaceId,
            cleanup: CleanupPlanId,
            prefix_cleanup: CleanupPlanId,
        },
        GenericCloneBorrow {
            borrow: BorrowId,
            cleanup: CleanupPlanId,
            prefix_cleanup: CleanupPlanId,
        },
        HandleAwareClonePlace {
            place: PlaceId,
            cleanup: CleanupPlanId,
            prefix_cleanup: CleanupPlanId,
        },
        HandleAwareCloneBorrow {
            borrow: BorrowId,
            cleanup: CleanupPlanId,
            prefix_cleanup: CleanupPlanId,
        },
        InitializePlace {
            place: PlaceId,
            value: ValueId,
        },
        ReplacePlace {
            place: PlaceId,
            value: ValueId,
        },
        GenericReplacePlace {
            place: PlaceId,
            value: ValueId,
        },
        DropPlace {
            place: PlaceId,
        },
        EnumDiscriminant {
            place: PlaceId,
        },
        FixedArrayIndexCopy {
            place: PlaceId,
            index: ValueId,
            cleanup: CleanupPlanId,
        },
        VecIndexCopy {
            place: PlaceId,
            index: ValueId,
            cleanup: CleanupPlanId,
        },
        StringFromUtf8 {
            bytes: Vec<u8>,
            cleanup: CleanupPlanId,
        },
        StringClone {
            place: PlaceId,
            cleanup: CleanupPlanId,
        },
        StringConcat {
            left: PlaceId,
            right: PlaceId,
            cleanup: CleanupPlanId,
        },
        VecClone {
            place: PlaceId,
            cleanup: CleanupPlanId,
            element_cleanup: Option<CleanupPlanId>,
        },
        VecConstruct {
            elements: Vec<ValueId>,
            cleanup: CleanupPlanId,
        },
        VecPush {
            vector: PlaceId,
            value: ValueId,
            cleanup: CleanupPlanId,
        },
        SharedConstruct {
            value: ValueId,
            cleanup: CleanupPlanId,
        },
        SharedClone {
            place: PlaceId,
            cleanup: CleanupPlanId,
        },
        WeakDowngrade {
            place: PlaceId,
            cleanup: CleanupPlanId,
        },
        WeakClone {
            place: PlaceId,
            cleanup: CleanupPlanId,
        },
        BeginBorrow(BorrowDefinition),
        BeginIndexedBorrow {
            definition: BorrowDefinition,
            index: ValueId,
            cleanup: CleanupPlanId,
        },
        BeginIndexedAccess {
            definition: BorrowDefinition,
            index: ValueId,
            cleanup: CleanupPlanId,
        },
        ProjectIndexedBorrow {
            parent: BorrowId,
            borrow: BorrowId,
            index: ValueId,
            cleanup: CleanupPlanId,
        },
        BindIndexedBorrow {
            parent: BorrowId,
            borrow: BorrowId,
        },
        BorrowReplace {
            borrow: BorrowId,
            value: ValueId,
        },
        BorrowRead {
            borrow: BorrowId,
        },
        BorrowWrite {
            borrow: BorrowId,
            value: ValueId,
        },
        EndBorrow {
            borrow: BorrowId,
        },
    }

    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct Block {
        pub id: BlockId,
        pub parameters: Vec<ValueDefinition>,
        pub instructions: Vec<Instruction>,
        pub terminators: Vec<SpannedTerminator>,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct Edge {
        pub target: BlockId,
        pub arguments: Vec<ValueId>,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct SpannedTerminator {
        pub span: Span,
        pub kind: Terminator,
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub enum Terminator {
        Return { value: ValueId, cleanup: CleanupPlanId },
        Jump(Edge),
        Branch { condition: ValueId, when_true: Edge, when_false: Edge },
        EnumMatch { place: PlaceId, arms: Vec<EnumArm> },
        WeakUpgradeBranch { weak: PlaceId, success: Edge, expired: Edge, cleanup: CleanupPlanId },
        Trap { identity: TrapIdentity, cleanup: CleanupPlanId },
    }
    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct EnumArm {
        pub variant: u32,
        pub edge: Edge,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
    pub enum TrapIdentity {
        BoundsV1,
        AllocationV1,
        CapacityV1,
        RefcountV1,
        Utf8V1,
    }

    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    pub struct CleanupPlan {
        pub id: CleanupPlanId,
        pub span: Span,
        pub actions: Vec<DropAction>,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
    pub enum DropAction {
        DropPlace(PlaceId),
        DropVecInitializedPrefix(PlaceId),
        DropAggregateInitializedPrefix(PlaceId),
        DropGenericCloneInitializedPrefix(PlaceId),
    }
}

/// Opaque identity of one verified program authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramIdentity {
    source_map: SourceMapIdentity,
    universe: TypeUniverseIdentity,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Opaque module identity branded by its verified program.
pub struct ModuleIdentity {
    owner: ProgramIdentity,
    index: u32,
}
#[allow(missing_docs)]
impl ModuleIdentity {
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Opaque function identity branded by its verified program.
pub struct FunctionIdentity {
    owner: ProgramIdentity,
    module: u32,
    declaration: u32,
}
#[allow(missing_docs)]
impl FunctionIdentity {
    #[must_use]
    pub const fn module(self) -> u32 {
        self.module
    }
    #[must_use]
    pub const fn declaration(self) -> u32 {
        self.declaration
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Opaque block identity branded by its containing function.
pub struct BlockIdentity {
    owner: FunctionIdentity,
    index: u32,
}
#[allow(missing_docs)]
impl BlockIdentity {
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Opaque value identity branded by its containing function.
pub struct ValueIdentity {
    owner: FunctionIdentity,
    index: u32,
}
#[allow(missing_docs)]
impl ValueIdentity {
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Opaque place identity branded by its containing function.
pub struct PlaceIdentity {
    owner: FunctionIdentity,
    index: u32,
}
#[allow(missing_docs)]
impl PlaceIdentity {
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Opaque borrow identity branded by its containing function.
pub struct BorrowIdentity {
    owner: FunctionIdentity,
    index: u32,
}
#[allow(missing_docs)]
impl BorrowIdentity {
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Opaque cleanup-plan identity branded by its containing function.
pub struct CleanupPlanIdentity {
    owner: FunctionIdentity,
    index: u32,
}
#[allow(missing_docs)]
impl CleanupPlanIdentity {
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
}
/// Closed reason an exact cleanup site may unwind owned values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedCleanupRole {
    /// A fallible operation failed before committing its result.
    PrepareFailure,
    /// One element clone failed after a destination Vec prefix was initialized.
    VecCloneElementFailure,
    /// One recursive leaf clone failed after an aggregate prefix was initialized.
    AggregateCloneElementFailure,
    /// Generic structural clone failed with a recursively partial destination.
    GenericClonePrefixFailure,
    /// A direct callee trapped after its by-value arguments were transferred.
    CallTrap,
    /// A function returned normally after transferring its result.
    Return,
    /// The program entered an explicit controlled trap.
    ControlledTrap,
}
/// Exact verified program point authorized to use one cleanup plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedCleanupSite {
    block: BlockIdentity,
    instruction: Option<u32>,
    role: VerifiedCleanupRole,
}
#[allow(missing_docs)]
impl VerifiedCleanupSite {
    #[must_use]
    pub const fn block(self) -> BlockIdentity {
        self.block
    }
    #[must_use]
    pub const fn instruction_index(self) -> Option<u32> {
        self.instruction
    }
    #[must_use]
    pub const fn role(self) -> VerifiedCleanupRole {
        self.role
    }
}

/// Program accepted only after authority, resource, identity, span, type, place, and CFG checks.
///
/// ```compile_fail
/// let _ = zryna_ir::data_ownership_v1::VerifiedProgram { program: todo!() };
/// ```
/// ```compile_fail
/// fn recover_raw(value: &zryna_ir::data_ownership_v1::VerifiedProgram) { let _ = &value.program; }
/// ```
#[derive(Clone, Debug)]
pub struct VerifiedProgram {
    program: raw::Program,
    identity: ProgramIdentity,
    linear32: VerifiedLayouts,
    linux_x86_64: VerifiedLayouts,
    abi: zryna_abi::VerifiedScalarAbiModule,
    abi_indices: Vec<Vec<Option<usize>>>,
    borrow_indices: Vec<Vec<BorrowIndex>>,
}

#[allow(missing_docs)]
impl VerifiedProgram {
    #[must_use]
    pub const fn identity(&self) -> ProgramIdentity {
        self.identity
    }
    #[must_use]
    pub const fn source_map_identity(&self) -> SourceMapIdentity {
        self.identity.source_map
    }
    #[must_use]
    pub const fn type_universe_identity(&self) -> TypeUniverseIdentity {
        self.identity.universe
    }
    #[must_use]
    pub const fn runtime_contract(&self) -> RuntimeContractIdentity {
        self.program.authorities.runtime
    }
    #[must_use]
    pub const fn linear32_layouts(&self) -> &VerifiedLayouts {
        &self.linear32
    }
    #[must_use]
    pub const fn linux_x86_64_layouts(&self) -> &VerifiedLayouts {
        &self.linux_x86_64
    }
    #[must_use]
    pub const fn scalar_abi(&self) -> &zryna_abi::VerifiedScalarAbiModule {
        &self.abi
    }
    #[must_use]
    pub fn modules(&self) -> impl ExactSizeIterator<Item = VerifiedModule<'_>> {
        self.program.modules.iter().enumerate().map(|(index, module)| VerifiedModule {
            owner: self,
            index,
            module,
        })
    }
}

#[derive(Clone, Copy, Debug)]
/// Immutable view of one verified module.
pub struct VerifiedModule<'a> {
    owner: &'a VerifiedProgram,
    index: usize,
    module: &'a raw::Module,
}
#[allow(missing_docs)]
impl<'a> VerifiedModule<'a> {
    #[must_use]
    pub const fn id(self) -> ModuleIdentity {
        ModuleIdentity { owner: self.owner.identity, index: self.module.id.0 }
    }
    #[must_use]
    pub const fn source_file(self) -> FileId {
        self.module.source_file
    }
    #[must_use]
    pub const fn data_declarations(self) -> u32 {
        self.module.data_declarations
    }
    #[must_use]
    pub fn functions(self) -> impl ExactSizeIterator<Item = VerifiedFunction<'a>> {
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

#[derive(Clone, Copy, Debug)]
/// Immutable view of one verified function.
pub struct VerifiedFunction<'a> {
    owner: &'a VerifiedProgram,
    module_index: usize,
    function_index: usize,
    function: &'a raw::Function,
}
#[allow(missing_docs)]
impl<'a> VerifiedFunction<'a> {
    fn borrows(self) -> &'a BorrowIndex {
        &self.owner.borrow_indices[self.module_index][self.function_index]
    }
    #[must_use]
    pub const fn id(self) -> FunctionIdentity {
        FunctionIdentity {
            owner: self.owner.identity,
            module: self.function.id.module.0,
            declaration: self.function.id.declaration,
        }
    }
    #[must_use]
    pub fn public_export(self) -> Option<zryna_abi::VerifiedScalarExport<'a>> {
        self.owner.abi_indices[self.module_index][self.function_index]
            .and_then(|index| self.owner.abi.exports().nth(index))
    }
    #[must_use]
    /// # Panics
    /// Panics only if retained verified layout authority is internally corrupted.
    pub fn result_type(self) -> LayoutTypeId {
        layout_type(&self.owner.linear32, self.function.result).expect("verified result type").id()
    }
    #[must_use]
    /// # Panics
    /// Panics only if retained verified layout authority is internally corrupted.
    pub fn parameters(self) -> impl ExactSizeIterator<Item = VerifiedValueDefinition> + 'a {
        let owner = self.id();
        let layouts = &self.owner.linear32;
        self.function.parameters.iter().map(move |value| VerifiedValueDefinition {
            id: ValueIdentity { owner, index: value.id.0 },
            ty: layout_type(layouts, value.ty).expect("verified parameter type").id(),
            span: value.span,
        })
    }
    #[must_use]
    /// # Panics
    /// Panics only if retained verified layout authority is internally corrupted.
    pub fn borrow_parameters(self) -> impl ExactSizeIterator<Item = VerifiedBorrowParameter> + 'a {
        let owner = self.id();
        let layouts = &self.owner.linear32;
        self.function.borrow_parameters.iter().map(move |borrow| VerifiedBorrowParameter {
            id: BorrowIdentity { owner, index: borrow.id.0 },
            referent: layout_type(layouts, borrow.referent).expect("verified borrow referent").id(),
            access: borrow.access.into(),
            span: borrow.span,
        })
    }
    #[must_use]
    pub fn blocks(self) -> impl ExactSizeIterator<Item = VerifiedBlock<'a>> {
        self.function.blocks.iter().enumerate().map(move |(block_index, block)| VerifiedBlock {
            function: self,
            block_index,
            block,
        })
    }
    #[must_use]
    pub fn places(self) -> impl ExactSizeIterator<Item = VerifiedPlace<'a>> {
        self.function.places.iter().map(move |place| VerifiedPlace { function: self, place })
    }
    #[must_use]
    pub fn cleanup_plans(self) -> impl ExactSizeIterator<Item = VerifiedCleanupPlan<'a>> {
        self.function
            .cleanup_plans
            .iter()
            .map(move |plan| VerifiedCleanupPlan { function: self, plan })
    }
}

#[derive(Clone, Copy, Debug)]
/// Immutable view of one verified basic block.
pub struct VerifiedBlock<'a> {
    function: VerifiedFunction<'a>,
    block_index: usize,
    block: &'a raw::Block,
}
#[allow(missing_docs)]
impl<'a> VerifiedBlock<'a> {
    #[must_use]
    pub const fn id(self) -> BlockIdentity {
        BlockIdentity { owner: self.function.id(), index: self.block.id.0 }
    }
    #[must_use]
    pub fn instructions(self) -> impl ExactSizeIterator<Item = VerifiedInstruction<'a>> {
        self.block.instructions.iter().enumerate().map(move |(instruction_index, instruction)| {
            VerifiedInstruction {
                function: self.function,
                block_index: self.block_index,
                instruction_index,
                instruction,
            }
        })
    }
    #[must_use]
    /// # Panics
    /// Panics only if retained verified layout authority is internally corrupted.
    pub fn parameters(self) -> impl ExactSizeIterator<Item = VerifiedValueDefinition> + 'a {
        let owner = self.function.id();
        let layouts = &self.function.owner.linear32;
        self.block.parameters.iter().map(move |value| VerifiedValueDefinition {
            id: ValueIdentity { owner, index: value.id.0 },
            ty: layout_type(layouts, value.ty).expect("verified block parameter type").id(),
            span: value.span,
        })
    }
    #[must_use]
    pub fn terminator(self) -> VerifiedTerminator<'a> {
        VerifiedTerminator {
            function: self.function,
            block_index: self.block_index,
            terminator: &self.block.terminators[0],
        }
    }
}
/// Sealed value definition exposed by a verified function or block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedValueDefinition {
    id: ValueIdentity,
    ty: LayoutTypeId,
    span: Span,
}
#[allow(missing_docs)]
impl VerifiedValueDefinition {
    #[must_use]
    pub const fn id(self) -> ValueIdentity {
        self.id
    }
    #[must_use]
    pub const fn ty(self) -> LayoutTypeId {
        self.ty
    }
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }
}
/// Sealed borrow access mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum VerifiedBorrowAccess {
    Shared,
    Exclusive,
}
impl From<raw::BorrowAccess> for VerifiedBorrowAccess {
    fn from(value: raw::BorrowAccess) -> Self {
        match value {
            raw::BorrowAccess::Shared => Self::Shared,
            raw::BorrowAccess::Exclusive => Self::Exclusive,
        }
    }
}
/// Sealed borrow parameter signature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedBorrowParameter {
    id: BorrowIdentity,
    referent: LayoutTypeId,
    access: VerifiedBorrowAccess,
    span: Span,
}
#[allow(missing_docs)]
impl VerifiedBorrowParameter {
    #[must_use]
    pub const fn id(self) -> BorrowIdentity {
        self.id
    }
    #[must_use]
    pub const fn referent(self) -> LayoutTypeId {
        self.referent
    }
    #[must_use]
    pub const fn access(self) -> VerifiedBorrowAccess {
        self.access
    }
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }
}
#[derive(Clone, Copy, Debug)]
/// Immutable view of one verified ownership place.
pub struct VerifiedPlace<'a> {
    function: VerifiedFunction<'a>,
    place: &'a raw::Place,
}
#[allow(missing_docs)]
impl VerifiedPlace<'_> {
    #[must_use]
    pub const fn id(self) -> PlaceIdentity {
        PlaceIdentity { owner: self.function.id(), index: self.place.id.0 }
    }
    /// # Panics
    ///
    /// Panics only if the verifier's retained sealed type authority is internally corrupted.
    #[must_use]
    pub fn ty(self) -> LayoutTypeId {
        layout_type(&self.function.owner.linear32, self.place.ty).expect("verified type").id()
    }
    /// Returns whether the sealed place type has no ownership/drop obligation.
    #[must_use]
    pub fn is_copy(self) -> bool {
        place_is_copy(self.place.id, self.function.function, &self.function.owner.linear32)
    }
    #[must_use]
    pub const fn span(self) -> Span {
        self.place.span
    }
    #[must_use]
    pub const fn kind(self) -> VerifiedPlaceKind {
        let owner = self.function.id();
        match self.place.kind {
            raw::PlaceKind::Parameter(index) => VerifiedPlaceKind::Parameter(index),
            raw::PlaceKind::Local(index) => VerifiedPlaceKind::Local(index),
            raw::PlaceKind::Temporary(value) => {
                VerifiedPlaceKind::Temporary(ValueIdentity { owner, index: value.0 })
            }
            raw::PlaceKind::StructField { base, ordinal } => VerifiedPlaceKind::StructField {
                base: PlaceIdentity { owner, index: base.0 },
                ordinal,
            },
            raw::PlaceKind::EnumPayload { base, variant } => VerifiedPlaceKind::EnumPayload {
                base: PlaceIdentity { owner, index: base.0 },
                variant,
            },
            raw::PlaceKind::FixedArrayConstant { base, index } => {
                VerifiedPlaceKind::FixedArrayConstant {
                    base: PlaceIdentity { owner, index: base.0 },
                    index,
                }
            }
        }
    }
}
/// Complete sealed place-root or projection identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum VerifiedPlaceKind {
    Parameter(u32),
    Local(u32),
    Temporary(ValueIdentity),
    StructField { base: PlaceIdentity, ordinal: u32 },
    EnumPayload { base: PlaceIdentity, variant: u32 },
    FixedArrayConstant { base: PlaceIdentity, index: u32 },
}
#[derive(Clone, Copy, Debug)]
/// Immutable view of one verified instruction.
///
/// Literal bytes are immutable through the verified view:
///
/// ```compile_fail
/// fn mutate(instruction: zryna_ir::data_ownership_v1::VerifiedInstruction<'_>) {
///     instruction.string_utf8_bytes().expect("String literal")[0] = b'x';
/// }
/// ```
pub struct VerifiedInstruction<'a> {
    function: VerifiedFunction<'a>,
    block_index: usize,
    instruction_index: usize,
    instruction: &'a raw::Instruction,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
/// Closed verified instruction opcode.
pub enum VerifiedInstructionKind {
    BoolLiteral,
    I32Literal,
    I32Add,
    I32Sub,
    I32Mul,
    I32Neg,
    Eq,
    Ne,
    I32LtS,
    I32LeS,
    I32GtS,
    I32GeS,
    DirectCall,
    StructConstruct,
    EnumConstruct,
    EnvironmentLookup,
    FixedArrayConstruct,
    CopyFromPlace,
    MoveFromPlace,
    GenericMoveFromPlace,
    ClonePlace,
    GenericClonePlace,
    GenericCloneBorrow,
    HandleAwareClonePlace,
    HandleAwareCloneBorrow,
    InitializePlace,
    ReplacePlace,
    GenericReplacePlace,
    DropPlace,
    EnumDiscriminant,
    FixedArrayIndexCopy,
    VecIndexCopy,
    StringFromUtf8,
    StringClone,
    StringConcat,
    VecClone,
    VecConstruct,
    VecPush,
    SharedConstruct,
    SharedClone,
    WeakDowngrade,
    WeakClone,
    BeginBorrow,
    BeginIndexedBorrow,
    BeginIndexedAccess,
    ProjectIndexedBorrow,
    BindIndexedBorrow,
    BorrowReplace,
    BorrowRead,
    BorrowWrite,
    EndBorrow,
}
#[allow(missing_docs)]
impl<'a> VerifiedInstruction<'a> {
    #[must_use]
    pub fn result(self) -> Option<ValueIdentity> {
        self.instruction
            .result
            .map(|result| ValueIdentity { owner: self.function.id(), index: result.id.0 })
    }
    #[must_use]
    pub const fn span(self) -> Span {
        self.instruction.span
    }
    #[must_use]
    pub const fn kind(self) -> VerifiedInstructionKind {
        use raw::InstructionKind as I;
        match &self.instruction.kind {
            I::BoolLiteral(_) => VerifiedInstructionKind::BoolLiteral,
            I::I32Literal(_) => VerifiedInstructionKind::I32Literal,
            I::I32Add { .. } => VerifiedInstructionKind::I32Add,
            I::I32Sub { .. } => VerifiedInstructionKind::I32Sub,
            I::I32Mul { .. } => VerifiedInstructionKind::I32Mul,
            I::I32Neg { .. } => VerifiedInstructionKind::I32Neg,
            I::Eq { .. } => VerifiedInstructionKind::Eq,
            I::Ne { .. } => VerifiedInstructionKind::Ne,
            I::I32LtS { .. } => VerifiedInstructionKind::I32LtS,
            I::I32LeS { .. } => VerifiedInstructionKind::I32LeS,
            I::I32GtS { .. } => VerifiedInstructionKind::I32GtS,
            I::I32GeS { .. } => VerifiedInstructionKind::I32GeS,
            I::DirectCall { .. } => VerifiedInstructionKind::DirectCall,
            I::StructConstruct { .. } => VerifiedInstructionKind::StructConstruct,
            I::EnumConstruct { .. } => VerifiedInstructionKind::EnumConstruct,
            I::EnvironmentLookup { .. } => VerifiedInstructionKind::EnvironmentLookup,
            I::FixedArrayConstruct { .. } => VerifiedInstructionKind::FixedArrayConstruct,
            I::CopyFromPlace { .. } => VerifiedInstructionKind::CopyFromPlace,
            I::MoveFromPlace { .. } => VerifiedInstructionKind::MoveFromPlace,
            I::GenericMoveFromPlace { .. } => VerifiedInstructionKind::GenericMoveFromPlace,
            I::ClonePlace { .. } => VerifiedInstructionKind::ClonePlace,
            I::GenericClonePlace { .. } => VerifiedInstructionKind::GenericClonePlace,
            I::GenericCloneBorrow { .. } => VerifiedInstructionKind::GenericCloneBorrow,
            I::HandleAwareClonePlace { .. } => VerifiedInstructionKind::HandleAwareClonePlace,
            I::HandleAwareCloneBorrow { .. } => VerifiedInstructionKind::HandleAwareCloneBorrow,
            I::InitializePlace { .. } => VerifiedInstructionKind::InitializePlace,
            I::ReplacePlace { .. } => VerifiedInstructionKind::ReplacePlace,
            I::GenericReplacePlace { .. } => VerifiedInstructionKind::GenericReplacePlace,
            I::DropPlace { .. } => VerifiedInstructionKind::DropPlace,
            I::EnumDiscriminant { .. } => VerifiedInstructionKind::EnumDiscriminant,
            I::FixedArrayIndexCopy { .. } => VerifiedInstructionKind::FixedArrayIndexCopy,
            I::VecIndexCopy { .. } => VerifiedInstructionKind::VecIndexCopy,
            I::StringFromUtf8 { .. } => VerifiedInstructionKind::StringFromUtf8,
            I::StringClone { .. } => VerifiedInstructionKind::StringClone,
            I::StringConcat { .. } => VerifiedInstructionKind::StringConcat,
            I::VecClone { .. } => VerifiedInstructionKind::VecClone,
            I::VecConstruct { .. } => VerifiedInstructionKind::VecConstruct,
            I::VecPush { .. } => VerifiedInstructionKind::VecPush,
            I::SharedConstruct { .. } => VerifiedInstructionKind::SharedConstruct,
            I::SharedClone { .. } => VerifiedInstructionKind::SharedClone,
            I::WeakDowngrade { .. } => VerifiedInstructionKind::WeakDowngrade,
            I::WeakClone { .. } => VerifiedInstructionKind::WeakClone,
            I::BeginBorrow(_) => VerifiedInstructionKind::BeginBorrow,
            I::BeginIndexedBorrow { .. } => VerifiedInstructionKind::BeginIndexedBorrow,
            I::BeginIndexedAccess { .. } => VerifiedInstructionKind::BeginIndexedAccess,
            I::ProjectIndexedBorrow { .. } => VerifiedInstructionKind::ProjectIndexedBorrow,
            I::BindIndexedBorrow { .. } => VerifiedInstructionKind::BindIndexedBorrow,
            I::BorrowReplace { .. } => VerifiedInstructionKind::BorrowReplace,
            I::BorrowRead { .. } => VerifiedInstructionKind::BorrowRead,
            I::BorrowWrite { .. } => VerifiedInstructionKind::BorrowWrite,
            I::EndBorrow { .. } => VerifiedInstructionKind::EndBorrow,
        }
    }
    /// Returns the separately sealed per-element failure cleanup for non-Copy Vec clone.
    #[must_use]
    pub const fn vec_clone_element_cleanup(self) -> Option<CleanupPlanIdentity> {
        let raw::InstructionKind::VecClone { element_cleanup: Some(cleanup), .. } =
            &self.instruction.kind
        else {
            return None;
        };
        Some(CleanupPlanIdentity { owner: self.function.id(), index: cleanup.0 })
    }
    /// Returns the separately sealed recursive-leaf failure cleanup for structural clone.
    #[must_use]
    pub const fn aggregate_clone_element_cleanup(self) -> Option<CleanupPlanIdentity> {
        let raw::InstructionKind::ClonePlace { element_cleanup: Some(cleanup), .. } =
            &self.instruction.kind
        else {
            return None;
        };
        Some(CleanupPlanIdentity { owner: self.function.id(), index: cleanup.0 })
    }
    /// Returns the exact number of fallible String-leaf clones for the selected root shape.
    ///
    /// Root enums require their authenticated active variant. Nested enums are outside this
    /// private structural-clone checkpoint.
    #[must_use]
    pub fn aggregate_clone_fallible_leaf_count(self) -> Option<u64> {
        let raw::InstructionKind::ClonePlace { place, element_cleanup: Some(_), .. } =
            &self.instruction.kind
        else {
            return None;
        };
        let ty = self.instruction.result?.ty;
        let ty = layout_type(&self.function.owner.linear32, ty)?.id();
        let category = self.function.owner.linear32.type_by_id(ty)?.category();
        let active_variant = if category == TypeCategory::Enum {
            let (_, variants) = derive_state_before(
                self.function.function,
                &self.function.owner.linear32,
                self.function.borrows(),
                self.block_index,
                self.instruction_index,
            )?;
            variants.get(place.0 as usize).copied().flatten()
        } else {
            None
        };
        aggregate_clone_fallible_leaf_count(ty, &self.function.owner.linear32, active_variant, true)
    }
    #[must_use]
    pub fn result_type(self) -> Option<LayoutTypeId> {
        self.instruction
            .result
            .and_then(|result| layout_type(&self.function.owner.linear32, result.ty))
            .map(zryna_layout::VerifiedType::id)
    }
    #[must_use]
    pub fn value_operands(self) -> impl ExactSizeIterator<Item = ValueIdentity> {
        let owner = self.function.id();
        instruction_operands(&self.instruction.kind)
            .into_iter()
            .map(move |value| ValueIdentity { owner, index: value.0 })
    }
    #[must_use]
    pub fn place_operands(self) -> impl ExactSizeIterator<Item = PlaceIdentity> {
        let owner = self.function.id();
        instruction_place_operands(&self.instruction.kind)
            .into_iter()
            .map(move |place| PlaceIdentity { owner, index: place.0 })
    }
    #[must_use]
    pub fn cleanup(self) -> Option<CleanupPlanIdentity> {
        instruction_cleanup(&self.instruction.kind)
            .map(|plan| CleanupPlanIdentity { owner: self.function.id(), index: plan.0 })
    }
    #[must_use]
    pub const fn bool_literal(self) -> Option<bool> {
        if let raw::InstructionKind::BoolLiteral(value) = &self.instruction.kind {
            Some(*value)
        } else {
            None
        }
    }
    #[must_use]
    pub const fn i32_literal(self) -> Option<i32> {
        if let raw::InstructionKind::I32Literal(value) = &self.instruction.kind {
            Some(*value)
        } else {
            None
        }
    }
    #[must_use]
    pub fn string_utf8_bytes(self) -> Option<&'a [u8]> {
        if let raw::InstructionKind::StringFromUtf8 { bytes, .. } = &self.instruction.kind {
            Some(bytes)
        } else {
            None
        }
    }
    #[must_use]
    pub const fn callee(self) -> Option<FunctionIdentity> {
        if let raw::InstructionKind::DirectCall { callee, .. } = &self.instruction.kind {
            Some(FunctionIdentity {
                owner: self.function.owner.identity,
                module: callee.module.0,
                declaration: callee.declaration,
            })
        } else {
            None
        }
    }
    #[must_use]
    pub fn call_arguments(self) -> impl ExactSizeIterator<Item = VerifiedCallArgument> + 'a {
        let owner = self.function.id();
        let arguments = match &self.instruction.kind {
            raw::InstructionKind::DirectCall { arguments, .. } => &arguments[..],
            _ => &[],
        };
        arguments.iter().map(move |argument| match argument {
            raw::CallArgument::Value(value) => {
                VerifiedCallArgument::Value(ValueIdentity { owner, index: value.0 })
            }
            raw::CallArgument::Borrow(borrow) => {
                VerifiedCallArgument::Borrow(BorrowIdentity { owner, index: borrow.0 })
            }
        })
    }
    #[must_use]
    pub const fn variant(self) -> Option<u32> {
        match &self.instruction.kind {
            raw::InstructionKind::EnumConstruct { variant, .. } => Some(*variant),
            _ => None,
        }
    }
    #[must_use]
    pub const fn borrow(self) -> Option<BorrowIdentity> {
        let id = match &self.instruction.kind {
            raw::InstructionKind::BeginBorrow(definition)
            | raw::InstructionKind::BeginIndexedBorrow { definition, .. }
            | raw::InstructionKind::BeginIndexedAccess { definition, .. } => definition.id,
            raw::InstructionKind::ProjectIndexedBorrow { borrow, .. }
            | raw::InstructionKind::BindIndexedBorrow { borrow, .. }
            | raw::InstructionKind::BorrowRead { borrow }
            | raw::InstructionKind::GenericCloneBorrow { borrow, .. }
            | raw::InstructionKind::HandleAwareCloneBorrow { borrow, .. }
            | raw::InstructionKind::BorrowWrite { borrow, .. }
            | raw::InstructionKind::BorrowReplace { borrow, .. }
            | raw::InstructionKind::EndBorrow { borrow } => *borrow,
            _ => return None,
        };
        Some(BorrowIdentity { owner: self.function.id(), index: id.0 })
    }
    #[must_use]
    pub const fn borrow_access(self) -> Option<VerifiedBorrowAccess> {
        match &self.instruction.kind {
            raw::InstructionKind::BeginBorrow(definition)
            | raw::InstructionKind::BeginIndexedBorrow { definition, .. }
            | raw::InstructionKind::BeginIndexedAccess { definition, .. } => {
                Some(match definition.access {
                    raw::BorrowAccess::Shared => VerifiedBorrowAccess::Shared,
                    raw::BorrowAccess::Exclusive => VerifiedBorrowAccess::Exclusive,
                })
            }
            _ => None,
        }
    }
    #[must_use]
    /// Returns every live owned root before a target storage allocation, in reverse completion order.
    pub fn allocation_failure_drop_actions(
        self,
    ) -> impl ExactSizeIterator<Item = VerifiedDropAction> {
        state_replay::construction_failure_actions(self).into_iter()
    }
    /// Returns exact semantic cleanup actions for this instruction.
    #[must_use]
    pub fn derived_drop_actions(self) -> impl ExactSizeIterator<Item = VerifiedDropAction> {
        let state = derive_state_before(
            self.function.function,
            &self.function.owner.linear32,
            self.function.borrows(),
            self.block_index,
            self.instruction_index,
        );
        let actions = state
            .map(|(states, variants)| match self.instruction.kind {
                raw::InstructionKind::DropPlace { place }
                    if root_place(place, self.function.function) == place =>
                {
                    vec![sealed_drop_action(
                        self.function.id(),
                        self.function.function,
                        place,
                        &states,
                        &variants,
                    )]
                }
                raw::InstructionKind::ReplacePlace { place, .. }
                | raw::InstructionKind::GenericReplacePlace { place, .. } => {
                    vec![sealed_drop_action(
                        self.function.id(),
                        self.function.function,
                        place,
                        &states,
                        &variants,
                    )]
                }
                _ => instruction_cleanup(&self.instruction.kind).map_or_else(Vec::new, |plan| {
                    sealed_drop_actions(
                        self.function.id(),
                        self.function.function,
                        plan,
                        &states,
                        &variants,
                    )
                }),
            })
            .unwrap_or_default();
        actions.into_iter()
    }
    /// Returns the sealed per-element failure cleanup for a non-Copy Vec clone.
    #[must_use]
    pub fn vec_clone_element_failure_drop_actions(
        self,
    ) -> impl ExactSizeIterator<Item = VerifiedDropAction> {
        let state = derive_state_before(
            self.function.function,
            &self.function.owner.linear32,
            self.function.borrows(),
            self.block_index,
            self.instruction_index,
        );
        let actions = state
            .and_then(|(states, variants)| {
                let raw::InstructionKind::VecClone { element_cleanup: Some(plan), .. } =
                    &self.instruction.kind
                else {
                    return None;
                };
                Some(sealed_drop_actions(
                    self.function.id(),
                    self.function.function,
                    *plan,
                    &states,
                    &variants,
                ))
            })
            .unwrap_or_default();
        actions.into_iter()
    }
    /// Returns the sealed recursive-prefix failure cleanup for structural aggregate clone.
    #[must_use]
    pub fn aggregate_clone_element_failure_drop_actions(
        self,
    ) -> impl ExactSizeIterator<Item = VerifiedDropAction> {
        let state = derive_state_before(
            self.function.function,
            &self.function.owner.linear32,
            self.function.borrows(),
            self.block_index,
            self.instruction_index,
        );
        let actions = state
            .and_then(|(states, mut variants)| {
                let raw::InstructionKind::ClonePlace { place, element_cleanup: Some(plan), .. } =
                    &self.instruction.kind
                else {
                    return None;
                };
                let result = self.instruction.result?;
                let result_owner = self
                    .function
                    .function
                    .places
                    .iter()
                    .find(|candidate| candidate.kind == raw::PlaceKind::Temporary(result.id))?
                    .id;
                variants[result_owner.0 as usize] = variants[place.0 as usize];
                Some(sealed_drop_actions(
                    self.function.id(),
                    self.function.function,
                    *plan,
                    &states,
                    &variants,
                ))
            })
            .unwrap_or_default();
        actions.into_iter()
    }
}
/// One sealed direct-call argument.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum VerifiedCallArgument {
    Value(ValueIdentity),
    Borrow(BorrowIdentity),
}
#[derive(Clone, Copy, Debug)]
/// Immutable view of one verified terminator.
pub struct VerifiedTerminator<'a> {
    function: VerifiedFunction<'a>,
    block_index: usize,
    terminator: &'a raw::SpannedTerminator,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
/// Closed verified terminator opcode.
pub enum VerifiedTerminatorKind {
    Return,
    Jump,
    Branch,
    EnumMatch,
    WeakUpgradeBranch,
    Trap,
}
/// One verified CFG edge with owner-branded target and arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedEdge {
    target: BlockIdentity,
    arguments: Vec<ValueIdentity>,
}
#[allow(missing_docs)]
impl VerifiedEdge {
    #[must_use]
    pub const fn target(&self) -> BlockIdentity {
        self.target
    }
    #[must_use]
    pub fn arguments(&self) -> impl ExactSizeIterator<Item = ValueIdentity> + '_ {
        self.arguments.iter().copied()
    }
}
#[allow(missing_docs)]
impl<'a> VerifiedTerminator<'a> {
    #[must_use]
    pub const fn span(self) -> Span {
        self.terminator.span
    }
    #[must_use]
    pub const fn kind(self) -> VerifiedTerminatorKind {
        match &self.terminator.kind {
            raw::Terminator::Return { .. } => VerifiedTerminatorKind::Return,
            raw::Terminator::Jump(_) => VerifiedTerminatorKind::Jump,
            raw::Terminator::Branch { .. } => VerifiedTerminatorKind::Branch,
            raw::Terminator::EnumMatch { .. } => VerifiedTerminatorKind::EnumMatch,
            raw::Terminator::WeakUpgradeBranch { .. } => VerifiedTerminatorKind::WeakUpgradeBranch,
            raw::Terminator::Trap { .. } => VerifiedTerminatorKind::Trap,
        }
    }
    #[must_use]
    pub const fn owner(self) -> FunctionIdentity {
        self.function.id()
    }
    #[must_use]
    pub fn value_operands(self) -> impl ExactSizeIterator<Item = ValueIdentity> {
        let owner = self.function.id();
        terminator_operands(&self.terminator.kind)
            .into_iter()
            .map(move |value| ValueIdentity { owner, index: value.0 })
    }
    #[must_use]
    pub fn place_operands(self) -> impl ExactSizeIterator<Item = PlaceIdentity> {
        let owner = self.function.id();
        terminator_place_operands(&self.terminator.kind)
            .into_iter()
            .map(move |place| PlaceIdentity { owner, index: place.0 })
    }
    #[must_use]
    pub fn cleanup(self) -> Option<CleanupPlanIdentity> {
        let (raw::Terminator::Return { cleanup, .. }
        | raw::Terminator::WeakUpgradeBranch { cleanup, .. }
        | raw::Terminator::Trap { cleanup, .. }) = &self.terminator.kind
        else {
            return None;
        };
        let plan = cleanup;
        Some(CleanupPlanIdentity { owner: self.function.id(), index: plan.0 })
    }
    #[must_use]
    pub const fn trap_identity(self) -> Option<VerifiedTrapIdentity> {
        let raw::Terminator::Trap { identity, .. } = &self.terminator.kind else { return None };
        Some(match identity {
            raw::TrapIdentity::BoundsV1 => VerifiedTrapIdentity::BoundsV1,
            raw::TrapIdentity::AllocationV1 => VerifiedTrapIdentity::AllocationV1,
            raw::TrapIdentity::CapacityV1 => VerifiedTrapIdentity::CapacityV1,
            raw::TrapIdentity::RefcountV1 => VerifiedTrapIdentity::RefcountV1,
            raw::TrapIdentity::Utf8V1 => VerifiedTrapIdentity::Utf8V1,
        })
    }
    #[must_use]
    pub fn enum_arms(self) -> impl ExactSizeIterator<Item = VerifiedEnumArm> + 'a {
        let owner = self.function.id();
        let arms = match &self.terminator.kind {
            raw::Terminator::EnumMatch { arms, .. } => &arms[..],
            _ => &[],
        };
        arms.iter().map(move |arm| VerifiedEnumArm {
            variant: arm.variant,
            edge: verified_edge(owner, &arm.edge),
        })
    }
    #[must_use]
    pub fn branch_edges(self) -> Option<(VerifiedEdge, VerifiedEdge)> {
        let raw::Terminator::Branch { when_true, when_false, .. } = &self.terminator.kind else {
            return None;
        };
        let owner = self.function.id();
        Some((verified_edge(owner, when_true), verified_edge(owner, when_false)))
    }
    #[must_use]
    pub fn edges(self) -> impl ExactSizeIterator<Item = VerifiedEdge> {
        let owner = self.function.id();
        terminator_edges(&self.terminator.kind).into_iter().map(move |edge| VerifiedEdge {
            target: BlockIdentity { owner, index: edge.target.0 },
            arguments: edge
                .arguments
                .iter()
                .map(|value| ValueIdentity { owner, index: value.0 })
                .collect(),
        })
    }
    #[must_use]
    pub fn derived_drop_actions(self) -> impl ExactSizeIterator<Item = VerifiedDropAction> {
        let actions = self.cleanup().and_then(|plan| {
            derive_state_before(
                self.function.function,
                &self.function.owner.linear32,
                self.function.borrows(),
                self.block_index,
                self.function.function.blocks[self.block_index].instructions.len(),
            )
            .map(|(states, variants)| {
                sealed_drop_actions(
                    self.function.id(),
                    self.function.function,
                    raw::CleanupPlanId(plan.index),
                    &states,
                    &variants,
                )
            })
        });
        actions.unwrap_or_default().into_iter()
    }
}
/// Executable sealed cleanup action derived for one exact cleanup site.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedDropAction {
    root: PlaceIdentity,
    kind: VerifiedDropActionKind,
    moved_projections: Vec<PlaceIdentity>,
    initialized_projections: Vec<PlaceIdentity>,
    active_variant: Option<u32>,
    active_variants: Vec<VerifiedActiveVariant>,
}
/// Closed cleanup behavior for one verified drop action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedDropActionKind {
    /// Recursively drop one fully or statically partially initialized owner.
    Place,
    /// Drop the runtime-recorded initialized Vec prefix in reverse, then release its storage.
    VecInitializedPrefix,
    /// Drop the runtime-recorded initialized structural prefix recursively.
    AggregateInitializedPrefix,
    /// Unwind the exact generic clone's runtime frontier, releasing storage after children.
    GenericCloneInitializedPrefix,
}
/// One exact active enum variant retained for recursive partial cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedActiveVariant {
    place: PlaceIdentity,
    variant: u32,
}
#[allow(missing_docs)]
impl VerifiedActiveVariant {
    #[must_use]
    pub const fn place(self) -> PlaceIdentity {
        self.place
    }
    #[must_use]
    pub const fn variant(self) -> u32 {
        self.variant
    }
}
#[allow(missing_docs)]
impl VerifiedDropAction {
    #[must_use]
    pub const fn root(&self) -> PlaceIdentity {
        self.root
    }
    #[must_use]
    pub const fn kind(&self) -> VerifiedDropActionKind {
        self.kind
    }
    #[must_use]
    pub fn moved_projections(&self) -> impl ExactSizeIterator<Item = PlaceIdentity> + '_ {
        self.moved_projections.iter().copied()
    }
    #[must_use]
    pub fn initialized_projections(&self) -> impl ExactSizeIterator<Item = PlaceIdentity> + '_ {
        self.initialized_projections.iter().copied()
    }
    #[must_use]
    pub const fn active_variant(&self) -> Option<u32> {
        self.active_variant
    }
    #[must_use]
    pub fn active_variants(&self) -> impl ExactSizeIterator<Item = VerifiedActiveVariant> + '_ {
        self.active_variants.iter().copied()
    }
}
/// Sealed trap ABI identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum VerifiedTrapIdentity {
    BoundsV1,
    AllocationV1,
    CapacityV1,
    RefcountV1,
    Utf8V1,
}
/// One role-preserving exhaustive enum arm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedEnumArm {
    variant: u32,
    edge: VerifiedEdge,
}
#[allow(missing_docs)]
impl VerifiedEnumArm {
    #[must_use]
    pub const fn variant(&self) -> u32 {
        self.variant
    }
    #[must_use]
    pub const fn edge(&self) -> &VerifiedEdge {
        &self.edge
    }
}

fn verified_edge(owner: FunctionIdentity, edge: &raw::Edge) -> VerifiedEdge {
    VerifiedEdge {
        target: BlockIdentity { owner, index: edge.target.0 },
        arguments: edge
            .arguments
            .iter()
            .map(|value| ValueIdentity { owner, index: value.0 })
            .collect(),
    }
}
#[derive(Clone, Copy, Debug)]
/// Immutable view of one verified cleanup plan.
pub struct VerifiedCleanupPlan<'a> {
    function: VerifiedFunction<'a>,
    plan: &'a raw::CleanupPlan,
}
#[allow(missing_docs)]
impl<'a> VerifiedCleanupPlan<'a> {
    #[must_use]
    pub const fn id(self) -> CleanupPlanIdentity {
        CleanupPlanIdentity { owner: self.function.id(), index: self.plan.id.0 }
    }
    #[must_use]
    pub fn actions(self) -> impl ExactSizeIterator<Item = PlaceIdentity> + 'a {
        self.plan.actions.iter().map(move |action| match action {
            raw::DropAction::DropPlace(place)
            | raw::DropAction::DropVecInitializedPrefix(place)
            | raw::DropAction::DropAggregateInitializedPrefix(place)
            | raw::DropAction::DropGenericCloneInitializedPrefix(place) => {
                PlaceIdentity { owner: self.function.id(), index: place.0 }
            }
        })
    }
    #[must_use]
    pub const fn span(self) -> Span {
        self.plan.span
    }
    /// Returns the single sealed site authorized to execute this plan.
    ///
    /// # Panics
    ///
    /// Panics only if the verifier's retained site binding is internally corrupted.
    #[must_use]
    pub fn site(self) -> VerifiedCleanupSite {
        let reference = cleanup_references(self.function.function)
            .into_iter()
            .find(|reference| reference.plan == self.plan.id)
            .expect("verified cleanup site");
        VerifiedCleanupSite {
            block: BlockIdentity {
                owner: self.function.id(),
                index: u32::try_from(reference.block).expect("bounded block index"),
            },
            instruction: reference
                .instruction
                .map(|index| u32::try_from(index).expect("bounded instruction index")),
            role: reference.role,
        }
    }
}

/// Verifies one raw M3 program against exact source and dual-target layout authorities.
///
/// # Errors
///
/// Returns deterministic bounded diagnostics and no partial authority when any claim fails.
pub fn verify(
    program: raw::Program,
    sources: &SourceMap,
    expected_entry: FileId,
    linear32: VerifiedLayouts,
    linux_x86_64: VerifiedLayouts,
) -> Result<VerifiedProgram, Vec<Diagnostic>> {
    verify_owned(program, sources, expected_entry, linear32, linux_x86_64, None)
}

pub(crate) fn verify_owned(
    program: raw::Program,
    sources: &SourceMap,
    expected_entry: FileId,
    linear32: VerifiedLayouts,
    linux_x86_64: VerifiedLayouts,
    command: Option<&zryna_syntax::command_h1_v1::CommandSyntax>,
) -> Result<VerifiedProgram, Vec<Diagnostic>> {
    let mut errors = Errors::default();
    let nominal_limit = MAX_NOMINAL_DECLARATIONS
        + usize::from(
            command.is_some_and(zryna_syntax::command_h1_v1::CommandSyntax::uses_outcome),
        );
    preflight_with_nominal_limit(&program, &linear32, nominal_limit, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let runtime = command.map_or(RuntimeContractIdentity::OwnershipRuntimeV1, |_| {
        RuntimeContractIdentity::CommandH1V1
    });
    verify_authorities(
        &program,
        sources,
        expected_entry,
        &linear32,
        &linux_x86_64,
        runtime,
        &mut errors,
    );
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    crate::command_h1_v1::admit_body(&program, sources, &linear32, &linux_x86_64, command)?;
    let mut borrow_indices = Vec::new();
    verify_structure(&program, sources, &linear32, &mut borrow_indices, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    verify_calls(&program, &borrow_indices, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let (abi, abi_indices) = verify_public_abi(&program, &linear32, &mut errors);
    if !errors.is_empty() {
        return Err(errors.finish());
    }
    let Some(abi) = abi else {
        return Err(vec![error(
            "ZRYNA-I3202",
            "DataOwnershipV1 verifier could not seal scalar ABI authority",
            "report the smallest reproducible compiler invariant failure",
        )]);
    };
    let identity =
        ProgramIdentity { source_map: sources.identity(), universe: linear32.universe_identity() };
    Ok(VerifiedProgram {
        program,
        identity,
        linear32,
        linux_x86_64,
        abi,
        abi_indices,
        borrow_indices,
    })
}

#[cfg(test)]
fn preflight(program: &raw::Program, layouts: &VerifiedLayouts, errors: &mut Errors) {
    preflight_with_nominal_limit(program, layouts, MAX_NOMINAL_DECLARATIONS, errors);
}

#[allow(clippy::too_many_lines)]
fn preflight_with_nominal_limit(
    program: &raw::Program,
    layouts: &VerifiedLayouts,
    nominal_limit: usize,
    errors: &mut Errors,
) {
    if program.modules.is_empty() || program.modules.len() > MAX_MODULES {
        errors.limit("module count", MAX_MODULES);
        return;
    }
    if layouts.types().len() > MAX_INSTANTIATED_TYPES {
        errors.limit("instantiated type count", MAX_INSTANTIATED_TYPES);
        return;
    }
    let mut functions = 0usize;
    let mut parameters = 0usize;
    let mut blocks = 0usize;
    let mut values = 0usize;
    let mut edges = 0usize;
    let mut aggregate_operands = 0usize;
    let mut string_literal_bytes = 0usize;
    let mut nominals = 0usize;
    let mut calls = 0usize;
    for module in &program.modules {
        nominals = checked_add(
            nominals,
            module.data_declarations as usize,
            "nominal declaration count",
            errors,
        );
        if nominals > nominal_limit {
            errors.limit("nominal declaration count", nominal_limit);
            return;
        }
        if module.functions.len() > MAX_FUNCTIONS_PER_MODULE {
            errors.limit("functions per module", MAX_FUNCTIONS_PER_MODULE);
            return;
        }
        functions = checked_add(functions, module.functions.len(), "function count", errors);
        if functions > MAX_FUNCTIONS_PER_PROGRAM {
            errors.limit("function count", MAX_FUNCTIONS_PER_PROGRAM);
            return;
        }
        for function in &module.functions {
            if function.parameters.len() > MAX_PARAMETERS_PER_FUNCTION {
                errors.limit("parameters per function", MAX_PARAMETERS_PER_FUNCTION);
                return;
            }
            if function.borrow_parameters.len() > MAX_ACTIVE_BORROWS_PER_FUNCTION {
                errors.limit("active borrows per function", MAX_ACTIVE_BORROWS_PER_FUNCTION);
                return;
            }
            parameters =
                checked_add(parameters, function.parameters.len(), "parameter count", errors);
            if parameters > MAX_PARAMETERS_PER_PROGRAM {
                errors.limit("parameter count", MAX_PARAMETERS_PER_PROGRAM);
                return;
            }
            if function.places.len() > MAX_PLACES_PER_FUNCTION {
                errors.limit("places per function", MAX_PLACES_PER_FUNCTION);
                return;
            }
            if function.cleanup_plans.len() > MAX_CLEANUP_PLANS_PER_FUNCTION {
                errors.limit("cleanup plans per function", MAX_CLEANUP_PLANS_PER_FUNCTION);
                return;
            }
            if function.blocks.is_empty() || function.blocks.len() > MAX_BLOCKS_PER_FUNCTION {
                errors.limit("blocks per function", MAX_BLOCKS_PER_FUNCTION);
                return;
            }
            blocks = checked_add(blocks, function.blocks.len(), "block count", errors);
            if blocks > MAX_BLOCKS_PER_PROGRAM {
                errors.limit("block count", MAX_BLOCKS_PER_PROGRAM);
                return;
            }
            let mut function_values = function.parameters.len();
            let mut transitions = 0usize;
            let mut function_edges = 0usize;
            let mut drops = 0usize;
            let mut peak_borrows = function.borrow_parameters.len();
            for plan in &function.cleanup_plans {
                drops = checked_add(drops, plan.actions.len(), "drop action count", errors);
            }
            if drops > MAX_DROP_ACTIONS_PER_FUNCTION {
                errors.limit("drop actions per function", MAX_DROP_ACTIONS_PER_FUNCTION);
                return;
            }
            for block in &function.blocks {
                let mut active_borrows = function.borrow_parameters.len();
                if block.parameters.len() > MAX_BLOCK_PARAMETERS {
                    errors.limit("block parameter count", MAX_BLOCK_PARAMETERS);
                    return;
                }
                function_values = checked_add(
                    function_values,
                    block.parameters.len()
                        + block.instructions.iter().filter(|i| i.result.is_some()).count(),
                    "value count",
                    errors,
                );
                if function_values > MAX_VALUES_PER_FUNCTION {
                    errors.limit("values per function", MAX_VALUES_PER_FUNCTION);
                    return;
                }
                transitions = checked_add(
                    transitions,
                    block.instructions.len(),
                    "ownership transition count",
                    errors,
                );
                for instruction in &block.instructions {
                    match instruction.kind {
                        raw::InstructionKind::BeginBorrow(_)
                        | raw::InstructionKind::BeginIndexedBorrow { .. }
                        | raw::InstructionKind::BeginIndexedAccess { .. } => {
                            active_borrows = active_borrows.saturating_add(1);
                            peak_borrows = peak_borrows.max(active_borrows);
                        }
                        raw::InstructionKind::EndBorrow { .. } => {
                            active_borrows = active_borrows.saturating_sub(1);
                        }
                        _ => {}
                    }
                    aggregate_operands = checked_add(
                        aggregate_operands,
                        aggregate_operand_count(&instruction.kind),
                        "aggregate operand count",
                        errors,
                    );
                    if let raw::InstructionKind::StringFromUtf8 { bytes, .. } = &instruction.kind {
                        string_literal_bytes = checked_add(
                            string_literal_bytes,
                            bytes.len(),
                            "String literal byte count",
                            errors,
                        );
                        if string_literal_bytes > MAX_STRING_LITERAL_BYTES {
                            errors.limit("String literal byte count", MAX_STRING_LITERAL_BYTES);
                            return;
                        }
                    }
                    if matches!(instruction.kind, raw::InstructionKind::DirectCall { .. }) {
                        calls = checked_add(calls, 1, "call edge count", errors);
                    }
                    if matches!(instruction.kind, raw::InstructionKind::DropPlace { .. }) {
                        drops = checked_add(drops, 1, "drop action count", errors);
                        if drops > MAX_DROP_ACTIONS_PER_FUNCTION {
                            errors
                                .limit("drop actions per function", MAX_DROP_ACTIONS_PER_FUNCTION);
                            return;
                        }
                    }
                }
                if transitions > MAX_OWNERSHIP_TRANSITIONS_PER_FUNCTION {
                    errors.limit(
                        "ownership transitions per function",
                        MAX_OWNERSHIP_TRANSITIONS_PER_FUNCTION,
                    );
                    return;
                }
                if block.terminators.len() == 1 {
                    function_edges = checked_add(
                        function_edges,
                        terminator_edge_count(&block.terminators[0].kind),
                        "CFG edges per function",
                        errors,
                    );
                }
            }
            if peak_borrows > MAX_ACTIVE_BORROWS_PER_FUNCTION {
                errors.limit(
                    "simultaneously active borrows per function",
                    MAX_ACTIVE_BORROWS_PER_FUNCTION,
                );
                return;
            }
            if function_edges > MAX_CFG_EDGES_PER_FUNCTION {
                errors.limit("CFG edges per function", MAX_CFG_EDGES_PER_FUNCTION);
                return;
            }
            values = checked_add(values, function_values, "value count", errors);
            edges = checked_add(edges, function_edges, "CFG edge count", errors);
            if values > MAX_VALUES_PER_PROGRAM
                || edges > MAX_CFG_EDGES_PER_PROGRAM
                || aggregate_operands > MAX_AGGREGATE_OPERANDS
                || calls > MAX_CALL_EDGES
            {
                errors.limit("program IR count", MAX_VALUES_PER_PROGRAM);
                return;
            }
        }
    }
}

fn verify_authorities(
    program: &raw::Program,
    sources: &SourceMap,
    expected_entry: FileId,
    linear: &VerifiedLayouts,
    linux: &VerifiedLayouts,
    runtime: RuntimeContractIdentity,
    errors: &mut Errors,
) {
    let claims = program.authorities;
    if claims.runtime != runtime
        || linear.source_map_identity() != sources.identity()
        || linux.source_map_identity() != sources.identity()
        || linear.target() != StorageTarget::Linear32V1
        || linux.target() != StorageTarget::LinuxX8664V1
        || linear.universe_identity() != linux.universe_identity()
        || claims.type_universe != linear.universe_identity().as_bytes()
        || &claims.linear32_fingerprint != linear.fingerprint()
        || &claims.linux_x86_64_fingerprint != linux.fingerprint()
        || !same_logical_universe(linear, linux)
    {
        errors.push(error(
            "ZRYNA-I3003",
            "DataOwnershipV1 layout or runtime authority does not match its sealed claim",
            "use both exact layouts issued from the final source-bound type universe",
        ));
        return;
    }
    let expected_nominals = program
        .modules
        .iter()
        .flat_map(|module| {
            (0..module.data_declarations).map(move |declaration| (module.id.0, declaration))
        })
        .collect::<BTreeSet<_>>();
    let layout_nominals = linear
        .types()
        .filter_map(zryna_layout::VerifiedType::nominal_identity)
        .collect::<BTreeSet<_>>();
    if expected_nominals != layout_nominals {
        errors.push(error(
            "ZRYNA-I3003",
            "module nominal declaration inventory does not match the sealed layout universe",
            "bind every source-ordered nominal declaration exactly once",
        ));
        return;
    }
    if program.modules.len() != sources.len() {
        errors.push(error(
            "ZRYNA-I3001",
            "module inventory does not match the final SourceMap",
            "provide every final module exactly once in canonical order",
        ));
    }
    let entry =
        usize::try_from(program.entry_module.0).ok().and_then(|index| program.modules.get(index));
    if entry.map(|module| module.source_file) != Some(expected_entry)
        || sources.source(expected_entry).is_none()
    {
        errors.push(error(
            "ZRYNA-I3001",
            "entry module is not bound to the independently supplied entry source",
            "use the exact entry FileId from the final SourceMap",
        ));
    }
}

#[allow(clippy::too_many_lines)]
fn verify_structure(
    program: &raw::Program,
    sources: &SourceMap,
    layouts: &VerifiedLayouts,
    borrow_indices: &mut Vec<Vec<BorrowIndex>>,
    errors: &mut Errors,
) {
    let generic_clone_types = generic_clone::classify_program(program, layouts);
    let handle_clone_types = handle_aware_clone::classify_program(program, layouts);
    for (module_index, module) in program.modules.iter().enumerate() {
        borrow_indices.push(Vec::new());
        if module.id.0 as usize != module_index
            || sources
                .verify_file_id(u32::try_from(module_index).expect("bounded module index"))
                .ok()
                != Some(module.source_file)
        {
            errors.push(error(
                "ZRYNA-I3002",
                "module has a noncanonical identity or source owner",
                "use dense modules in final SourceMap order",
            ));
        }
        for (function_index, function) in module.functions.iter().enumerate() {
            if function.id.module != module.id || function.id.declaration as usize != function_index
            {
                errors.push(error_at(
                    "ZRYNA-I3002",
                    function.span,
                    "function has a noncanonical owner or declaration identity",
                    "use its containing module and dense declaration index",
                ));
            }
            check_span(function.span, module.source_file, sources, errors);
            if layout_type(layouts, function.result).is_none() {
                errors.push(error_at(
                    "ZRYNA-I3003",
                    function.span,
                    "function result names an unknown layout TypeId",
                    "use a type from the exact sealed universe",
                ));
            }
            let mut next_value = 0u32;
            let mut next_borrow = 0u32;
            for value in &function.parameters {
                check_value(value, &mut next_value, module.source_file, sources, layouts, errors);
            }
            for borrow in &function.borrow_parameters {
                if borrow.id.0 != next_borrow || layout_type(layouts, borrow.referent).is_none() {
                    errors.push(error_at(
                        "ZRYNA-I3011",
                        borrow.span,
                        "borrow parameter has a noncanonical identity or unknown referent type",
                        "use dense borrow parameters bound to sealed types",
                    ));
                }
                next_borrow = next_borrow.saturating_add(1);
                check_span(borrow.span, module.source_file, sources, errors);
            }
            let mut root_keys = BTreeSet::new();
            let mut projection_keys = BTreeSet::new();
            for (place_index, place) in function.places.iter().enumerate() {
                if place.id.0 as usize != place_index || layout_type(layouts, place.ty).is_none() {
                    errors.push(error_at(
                        "ZRYNA-I3006",
                        place.span,
                        "place has a noncanonical identity or unknown type",
                        "use dense places and sealed layout types",
                    ));
                }
                let root = match place.kind {
                    raw::PlaceKind::Parameter(index) => Some((0u8, index)),
                    raw::PlaceKind::Local(index) => Some((1u8, index)),
                    raw::PlaceKind::Temporary(value) => Some((2u8, value.0)),
                    _ => None,
                };
                if root.is_some_and(|key| !root_keys.insert(key)) {
                    errors.push(error_at(
                        "ZRYNA-I3006",
                        place.span,
                        "place root aliases an existing parameter, local, or temporary owner",
                        "declare exactly one root place for each owned storage identity",
                    ));
                }
                let projection = match place.kind {
                    raw::PlaceKind::StructField { base, ordinal } => Some((0u8, base.0, ordinal)),
                    raw::PlaceKind::EnumPayload { base, variant } => Some((1u8, base.0, variant)),
                    raw::PlaceKind::FixedArrayConstant { base, index } => {
                        Some((2u8, base.0, index))
                    }
                    _ => None,
                };
                if projection.is_some_and(|key| !projection_keys.insert(key)) {
                    errors.push(error_at(
                        "ZRYNA-I3006",
                        place.span,
                        "projection aliases an existing base and selector",
                        "declare each canonical field, payload, or array projection once",
                    ));
                }
                let mapped_type = match place.kind {
                    raw::PlaceKind::Parameter(index) => {
                        function.parameters.get(index as usize).map(|value| value.ty)
                    }
                    raw::PlaceKind::Temporary(value) => function_value_type(function, value),
                    _ => Some(place.ty),
                };
                if mapped_type != Some(place.ty) {
                    errors.push(error_at(
                        "ZRYNA-I3006",
                        place.span,
                        "root place does not exactly match its parameter or temporary value type",
                        "bind roots only to existing values of the same sealed type",
                    ));
                }
                check_span(place.span, module.source_file, sources, errors);
                verify_projection(place, function, layouts, errors);
            }
            for (plan_index, plan) in function.cleanup_plans.iter().enumerate() {
                let mut dropped = BTreeSet::new();
                if plan.id.0 as usize != plan_index
                    || plan.actions.iter().any(|a| match a {
                        raw::DropAction::DropPlace(p)
                        | raw::DropAction::DropVecInitializedPrefix(p)
                        | raw::DropAction::DropAggregateInitializedPrefix(p)
                        | raw::DropAction::DropGenericCloneInitializedPrefix(p) => {
                            p.0 as usize >= function.places.len() || !dropped.insert(*p)
                        }
                    })
                {
                    errors.push(error_at(
                        "ZRYNA-I3012",
                        plan.span,
                        "cleanup plan has a noncanonical identity or foreign place",
                        "use dense plans containing each local verified place at most once",
                    ));
                }
                check_span(plan.span, module.source_file, sources, errors);
            }
            for (block_index, block) in function.blocks.iter().enumerate() {
                if block.id.0 as usize != block_index {
                    errors.push(error_at(
                        "ZRYNA-I3002",
                        function.span,
                        "block identity is not dense",
                        "use dense block identities in arena order",
                    ));
                }
                if block_index == 0 && !block.parameters.is_empty() {
                    errors.push(error_at(
                        "ZRYNA-I3007",
                        function.span,
                        "entry block declares block parameters",
                        "use function parameters for entry values",
                    ));
                }
                for value in &block.parameters {
                    check_value(
                        value,
                        &mut next_value,
                        module.source_file,
                        sources,
                        layouts,
                        errors,
                    );
                }
                for instruction in &block.instructions {
                    check_span(instruction.span, module.source_file, sources, errors);
                    if let Some(result) = &instruction.result {
                        check_value(
                            result,
                            &mut next_value,
                            module.source_file,
                            sources,
                            layouts,
                            errors,
                        );
                    }
                    if let raw::InstructionKind::BeginBorrow(definition)
                    | raw::InstructionKind::BeginIndexedBorrow { definition, .. }
                    | raw::InstructionKind::BeginIndexedAccess { definition, .. } =
                        &instruction.kind
                    {
                        if definition.id.0 != next_borrow {
                            errors.push(error_at(
                                "ZRYNA-I3011",
                                definition.span,
                                "borrow definitions are not in canonical dense order",
                                "allocate borrow parameters then lexical borrows in instruction order",
                            ));
                        }
                        next_borrow = next_borrow.saturating_add(1);
                        check_span(definition.span, module.source_file, sources, errors);
                    }
                    if let raw::InstructionKind::ProjectIndexedBorrow { borrow, .. }
                    | raw::InstructionKind::BindIndexedBorrow { borrow, .. } = instruction.kind
                    {
                        if borrow.0 != next_borrow {
                            errors.push(error_at("ZRYNA-I3011", instruction.span,
                                "borrow definitions are not in canonical dense order",
                                "allocate borrow parameters then lexical borrows in instruction order"));
                        }
                        next_borrow = next_borrow.saturating_add(1);
                    }
                    verify_instruction_shape(instruction, function, errors);
                }
                if block.terminators.len() != 1 {
                    errors.push(error_at(
                        "ZRYNA-I3007",
                        function.span,
                        "block does not contain exactly one terminator",
                        "emit exactly one closed DataOwnershipV1 terminator",
                    ));
                }
                if let Some(terminator) = block.terminators.first() {
                    check_span(terminator.span, module.source_file, sources, errors);
                    verify_terminator(terminator, function, errors);
                }
            }
            let cleanup_references = cleanup_references(function);
            if let Some(plan) = function.cleanup_plans.iter().find(|plan| {
                cleanup_references.iter().filter(|reference| reference.plan == plan.id).count() != 1
            }) {
                errors.push(error_at(
                    "ZRYNA-I3012",
                    plan.span,
                    "cleanup plan is not bound to exactly one operation or exit site",
                    "create one dense cleanup plan for each exact prepare, call, return, or trap site",
                ));
            }
            verify_borrow_parameter_usage(function, errors);
            if !errors.is_empty() {
                return;
            }
            let mut borrows = BorrowIndex::new(function, layouts);
            verify_function_graph(
                function,
                layouts,
                &generic_clone_types,
                &handle_clone_types,
                &mut borrows,
                errors,
            );
            borrow_indices[module_index].push(borrows);
            if !errors.is_empty() {
                return;
            }
        }
    }
}

fn verify_borrow_parameter_usage(function: &raw::Function, errors: &mut Errors) {
    let mut used = vec![
        false;
        function.borrow_parameters.len().saturating_add(
            function.blocks.iter().map(|block| block.instructions.len()).sum::<usize>(),
        )
    ];
    for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
        match &instruction.kind {
            raw::InstructionKind::BorrowRead { borrow }
            | raw::InstructionKind::GenericCloneBorrow { borrow, .. }
            | raw::InstructionKind::HandleAwareCloneBorrow { borrow, .. }
            | raw::InstructionKind::BorrowWrite { borrow, .. }
            | raw::InstructionKind::BorrowReplace { borrow, .. } => {
                if let Some(slot) = used.get_mut(borrow.0 as usize) {
                    *slot = true;
                }
            }
            raw::InstructionKind::DirectCall { arguments, .. } => {
                for borrow in arguments.iter().filter_map(|argument| match argument {
                    raw::CallArgument::Borrow(borrow) => Some(*borrow),
                    raw::CallArgument::Value(_) => None,
                }) {
                    if let Some(slot) = used.get_mut(borrow.0 as usize) {
                        *slot = true;
                    }
                }
            }
            _ => {}
        }
    }
    for parameter in &function.borrow_parameters {
        if !used.get(parameter.id.0 as usize).copied().unwrap_or(false) {
            errors.push(error_at(
                "ZRYNA-I3011",
                parameter.span,
                "borrow parameter is not used by an access or direct call",
                "use every borrow authority through BorrowRead, BorrowWrite, BorrowReplace, or one exact borrow argument",
            ));
        }
    }
}

#[derive(Clone, Copy)]
enum DefinitionSite {
    FunctionParameter,
    BlockParameter(usize),
    Instruction(usize, usize),
}

#[derive(Clone, Copy)]
struct ValueInfo {
    ty: raw::TypeId,
    site: DefinitionSite,
}

#[allow(clippy::too_many_lines)]
fn verify_function_graph(
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    generic_clone_types: &[bool],
    handle_clone_types: &[bool],
    borrows: &mut BorrowIndex,
    errors: &mut Errors,
) {
    if function.blocks.is_empty() {
        return;
    }
    let mut values = Vec::new();
    values.extend(
        function
            .parameters
            .iter()
            .map(|value| ValueInfo { ty: value.ty, site: DefinitionSite::FunctionParameter }),
    );
    for (block_index, block) in function.blocks.iter().enumerate() {
        values.extend(block.parameters.iter().map(|value| ValueInfo {
            ty: value.ty,
            site: DefinitionSite::BlockParameter(block_index),
        }));
        values.extend(block.instructions.iter().enumerate().filter_map(
            |(instruction_index, instruction)| {
                instruction.result.map(|result| ValueInfo {
                    ty: result.ty,
                    site: DefinitionSite::Instruction(block_index, instruction_index),
                })
            },
        ));
    }

    let mut successors = vec![Vec::new(); function.blocks.len()];
    let mut predecessors = vec![Vec::new(); function.blocks.len()];
    for (block_index, block) in function.blocks.iter().enumerate() {
        let Some(terminator) = block.terminators.first() else {
            continue;
        };
        for edge in terminator_edges(&terminator.kind) {
            let Ok(target) = usize::try_from(edge.target.0) else {
                continue;
            };
            let Some(target_block) = function.blocks.get(target) else {
                continue;
            };
            successors[block_index].push(target);
            predecessors[target].push(block_index);
            let synthesized = matches!(
                &terminator.kind,
                raw::Terminator::WeakUpgradeBranch { success, .. }
                    if std::ptr::eq(edge, success)
            );
            let target_parameters = if synthesized {
                &target_block.parameters[usize::from(!target_block.parameters.is_empty())..]
            } else {
                &target_block.parameters[..]
            };
            if edge.arguments.len() != target_parameters.len() {
                errors.push(error_at(
                    "ZRYNA-I3007",
                    terminator.span,
                    "CFG edge argument arity does not match target block parameters",
                    "pass one exact typed value for every target block parameter",
                ));
                continue;
            }
            for (argument, parameter) in edge.arguments.iter().zip(target_parameters) {
                if value_info(&values, *argument).is_none_or(|info| info.ty != parameter.ty) {
                    errors.push(error_at(
                        "ZRYNA-I3007",
                        terminator.span,
                        "CFG edge argument type does not match its target parameter",
                        "pass exact sealed types across every edge",
                    ));
                }
            }
        }
    }
    if !predecessors[0].is_empty() {
        errors.push(error_at(
            "ZRYNA-I3007",
            function.span,
            "entry block has a predecessor",
            "never target the entry block",
        ));
    }
    let mut reachable = vec![false; function.blocks.len()];
    let mut queue = VecDeque::from([0usize]);
    while let Some(block) = queue.pop_front() {
        if reachable[block] {
            continue;
        }
        reachable[block] = true;
        queue.extend(successors[block].iter().copied());
    }
    if reachable.iter().any(|value| !value) || predecessors.iter().skip(1).any(Vec::is_empty) {
        errors.push(error_at(
            "ZRYNA-I3007",
            function.span,
            "function contains an unreachable or predecessor-free nonentry block",
            "emit only blocks reachable from entry",
        ));
    }

    let dominators = compute_dominators(&predecessors, &reachable);
    verify_reducible_loops(&successors, &predecessors, &dominators, Some(function.span), errors);
    if !errors.is_empty() {
        return;
    }
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            for operand in instruction_operands(&instruction.kind) {
                verify_use(
                    operand,
                    block_index,
                    instruction_index,
                    instruction.span,
                    &values,
                    &dominators,
                    errors,
                );
            }
            verify_operation_types(
                instruction,
                function,
                &values,
                layouts,
                (generic_clone_types, handle_clone_types),
                borrows,
                errors,
            );
        }
        if let Some(terminator) = block.terminators.first() {
            for operand in terminator_operands(&terminator.kind) {
                verify_use(
                    operand,
                    block_index,
                    block.instructions.len(),
                    terminator.span,
                    &values,
                    &dominators,
                    errors,
                );
            }
            verify_terminator_types(terminator, function, &values, layouts, errors);
        }
    }
    let enum_payloads = classify_enum_payload_projections(function);
    verify_enum_payload_move_contexts(function, layouts, &enum_payloads, errors);
    verify_aggregate_subobject_move_contexts(function, layouts, &enum_payloads, errors);
    verify_projected_aggregate_clone_contexts(function, layouts, errors);
    verify_projected_aggregate_assignment_contexts(function, layouts, errors);
    if !errors.is_empty() {
        return;
    }
    let value_owners = derive_value_owners(function, &values, layouts, errors);
    if !errors.is_empty() {
        return;
    }
    borrows.incoming = transient_edges::derive(function, &successors, &dominators, borrows, errors);
    if errors.is_empty() {
        verify_ownership_dataflow(function, &value_owners, layouts, &successors, borrows, errors);
    }
}

fn static_projection_path(mut place: raw::PlaceId, function: &raw::Function) -> bool {
    let mut projected = false;
    let mut visited = BTreeSet::new();
    while visited.insert(place) {
        let Some(record) = function.places.get(place.0 as usize) else { return false };
        match record.kind {
            raw::PlaceKind::StructField { base, .. }
            | raw::PlaceKind::FixedArrayConstant { base, .. } => {
                projected = true;
                place = base;
            }
            raw::PlaceKind::Parameter(_)
            | raw::PlaceKind::Local(_)
            | raw::PlaceKind::Temporary(_) => return projected,
            raw::PlaceKind::EnumPayload { .. } => return false,
        }
    }
    false
}

fn classify_enum_payload_projections(
    function: &raw::Function,
) -> Vec<Option<(raw::PlaceId, raw::PlaceId, u32)>> {
    let mut classifications = vec![None; function.places.len()];
    for place in &function.places {
        classifications[place.id.0 as usize] = match place.kind {
            raw::PlaceKind::StructField { base, .. }
            | raw::PlaceKind::FixedArrayConstant { base, .. } => {
                classifications.get(base.0 as usize).copied().flatten()
            }
            raw::PlaceKind::EnumPayload { base, variant } => Some((place.id, base, variant)),
            raw::PlaceKind::Parameter(_)
            | raw::PlaceKind::Local(_)
            | raw::PlaceKind::Temporary(_) => None,
        };
    }
    classifications
}

#[allow(clippy::too_many_lines)]
fn verify_enum_payload_move_contexts(
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    enum_payloads: &[Option<(raw::PlaceId, raw::PlaceId, u32)>],
    errors: &mut Errors,
) {
    let mut moves = Vec::with_capacity(2);
    'blocks: for (block_index, block) in function.blocks.iter().enumerate() {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let raw::InstructionKind::MoveFromPlace { place } = instruction.kind else {
                continue;
            };
            let Some(result) = instruction.result else { continue };
            let aggregate = layout_type(layouts, result.ty).is_some_and(|ty| {
                matches!(ty.category(), TypeCategory::Struct | TypeCategory::FixedArray)
            });
            let projection = enum_payloads.get(place.0 as usize).copied().flatten();
            if aggregate && let Some(projection) = projection {
                moves.push((block_index, block, index, instruction, place, result, projection));
                if moves.len() == 2 {
                    break 'blocks;
                }
            }
        }
    }
    if let Some((_, _, _, instruction, _, _, _)) = moves.get(1) {
        errors.push(error_at(
            "ZRYNA-I3010",
            instruction.span,
            "function contains more than one enum payload move",
            "move at most one complete active Struct or fixed-array payload through its exact canonical enum arm",
        ));
        return;
    }
    let Some((arm_index, arm, instruction_index, instruction, source, result, projection)) =
        moves.first().copied()
    else {
        return;
    };
    let (payload, enum_root, variant) = projection;
    let valid_payload_type = layout_type(layouts, result.ty)
        .is_some_and(|ty| matches!(ty.category(), TypeCategory::Struct | TypeCategory::FixedArray));
    let enum_type =
        function.places.get(enum_root.0 as usize).and_then(|place| layout_type(layouts, place.ty));
    let one_exact_variant = enum_type.is_some_and(|ty| {
        ty.category() == TypeCategory::Enum
            && ty.variants().len() == 1
            && ty.variants()[0].ordinal() == variant
            && ty.variants()[0].payload().is_some_and(|ty| ty.index() == result.ty.0)
    });
    let root_is_parameter = function.places.get(enum_root.0 as usize).is_some_and(|place| {
        matches!(place.kind, raw::PlaceKind::Parameter(parameter) if function.parameters.get(parameter as usize).is_some_and(|value| value.ty == place.ty))
    });
    let move_owners = function
        .places
        .iter()
        .filter(|candidate| {
            candidate.ty == result.ty
                && matches!(candidate.kind, raw::PlaceKind::Temporary(value) if value == result.id)
        })
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    let move_owner = (move_owners.len() == 1).then(|| move_owners[0]);
    let direct_local = arm.instructions.get(instruction_index + 1).and_then(|next| {
        let raw::InstructionKind::InitializePlace { place: target, value } = next.kind else {
            return None;
        };
        (value == result.id
            && function.places.get(target.0 as usize).is_some_and(|target| {
                target.ty == result.ty && matches!(target.kind, raw::PlaceKind::Local(_))
            }))
        .then_some(target)
    });
    let exact_drop = arm.instructions.get(instruction_index + 2).is_some_and(
        |next| matches!(next.kind, raw::InstructionKind::DropPlace { place } if place == enum_root),
    );
    let value_uses = function
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .instructions
                .iter()
                .flat_map(|candidate| instruction_operands(&candidate.kind))
                .chain(
                    block
                        .terminators
                        .iter()
                        .flat_map(|terminator| terminator_operands(&terminator.kind)),
                )
        })
        .filter(|value| *value == result.id)
        .count();
    let entry = function.blocks.first();
    let canonical_entry = entry.is_some_and(|entry| {
        entry.instructions.is_empty()
            && entry.terminators.len() == 1
            && entry.terminators.first().is_some_and(|terminator| {
                matches!(
                    &terminator.kind,
                    raw::Terminator::EnumMatch { place, arms }
                        if *place == enum_root
                            && arms.len() == 1
                            && arms[0].variant == variant
                            && arms[0].edge.target.0 as usize == arm_index
                            && arms[0].edge.arguments.is_empty()
                )
            })
    });
    let continuation_index = arm.terminators.first().and_then(|terminator| {
        let raw::Terminator::Jump(edge) = &terminator.kind else { return None };
        edge.arguments.is_empty().then_some(edge.target.0 as usize)
    });
    let canonical_continuation = direct_local.zip(continuation_index).and_then(
        |(local, continuation_index)| {
            let continuation = function.blocks.get(continuation_index)?;
            let [final_move] = continuation.instructions.as_slice() else { return None };
            let raw::InstructionKind::MoveFromPlace { place } = final_move.kind else {
                return None;
            };
            let final_result = final_move.result?;
            let final_owners = function
                .places
                .iter()
                .filter(|candidate| {
                    candidate.ty == final_result.ty
                        && matches!(candidate.kind, raw::PlaceKind::Temporary(value) if value == final_result.id)
                })
                .map(|candidate| candidate.id)
                .collect::<Vec<_>>();
            let final_owner = (final_owners.len() == 1).then(|| final_owners[0])?;
            let exact_return = continuation.terminators.first().is_some_and(|terminator| {
                let raw::Terminator::Return { value, cleanup } = terminator.kind else {
                    return false;
                };
                value == final_result.id
                    && function
                        .cleanup_plans
                        .get(cleanup.0 as usize)
                        .is_some_and(|plan| plan.actions.is_empty())
            });
            (place == local
                && continuation.terminators.len() == 1
                && final_result.ty == result.ty
                && function.result == result.ty
                && exact_return)
                .then_some((final_result, final_owner))
        },
    );
    let exact_parameter = function.parameters.as_slice().first().is_some_and(|parameter| {
        function.parameters.len() == 1
            && parameter.id == raw::ValueId(0)
            && function.places.get(enum_root.0 as usize).is_some_and(|place| {
                place.ty == parameter.ty && matches!(place.kind, raw::PlaceKind::Parameter(0))
            })
    });
    let instruction_results = function
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter().filter_map(|instruction| instruction.result))
        .collect::<Vec<_>>();
    let exact_values = instruction_results.len() == 2
        && instruction_results[0] == result
        && canonical_continuation
            .is_some_and(|(final_result, _)| instruction_results[1] == final_result);
    let source_descendants = function
        .places
        .iter()
        .filter(|place| is_projection_below(place.id, source, &function.places))
        .map(|place| place.id)
        .collect::<BTreeSet<_>>();
    let exact_places = move_owner.zip(direct_local).zip(canonical_continuation).is_some_and(
        |((move_owner, local), (_, final_owner))| {
            let mut expected = source_descendants.clone();
            expected.extend([enum_root, source, move_owner, local, final_owner]);
            function.places.len() == source_descendants.len() + 5
                && expected.len() == function.places.len()
                && function.places.iter().all(|place| expected.contains(&place.id))
        },
    );
    let exact_cleanup_metadata = function.cleanup_plans.len() == 1
        && function.cleanup_plans[0].id == raw::CleanupPlanId(0)
        && function.cleanup_plans[0].actions.is_empty();
    let exact_shape = function.entry_export.is_none()
        && function.borrow_parameters.is_empty()
        && function.blocks.len() == 3
        && function.blocks.iter().all(|block| block.parameters.is_empty())
        && arm_index != 0
        && arm.instructions.len() == 3
        && arm.terminators.len() == 1
        && instruction_index == 0
        && continuation_index.is_some_and(|index| index != 0 && index != arm_index);
    if !exact_shape
        || source != payload
        || !valid_payload_type
        || !one_exact_variant
        || !root_is_parameter
        || !exact_parameter
        || !canonical_entry
        || move_owner.is_none()
        || direct_local.is_none()
        || !exact_drop
        || value_uses != 1
        || !has_exact_projection_topology(source, function, layouts)
        || canonical_continuation.is_none()
        || !exact_values
        || !exact_places
        || !exact_cleanup_metadata
    {
        errors.push(error_at(
            "ZRYNA-I3010",
            instruction.span,
            "enum payload move escapes its exact canonical arm",
            "move one complete active Struct or fixed-array payload into the immediately following same-type local, drop its one-variant enum root, and jump without ownership arguments to an exact local return",
        ));
    }
}

#[allow(clippy::too_many_lines)]
fn verify_aggregate_subobject_move_contexts(
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    enum_payloads: &[Option<(raw::PlaceId, raw::PlaceId, u32)>],
    errors: &mut Errors,
) {
    let private_straight_line = function.entry_export.is_none()
        && function.blocks.len() == 1
        && function
            .blocks
            .first()
            .and_then(|block| block.terminators.first())
            .is_some_and(|terminator| terminator_edges(&terminator.kind).is_empty());
    let mut moves = Vec::with_capacity(2);
    'blocks: for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let raw::InstructionKind::MoveFromPlace { place } = instruction.kind else {
                continue;
            };
            let Some(result) = instruction.result else { continue };
            let Some(source) = function.places.get(place.0 as usize) else { continue };
            if projection_base(&source.kind).is_some()
                && enum_payloads.get(place.0 as usize).is_none_or(Option::is_none)
                && layout_type(layouts, result.ty).is_some_and(|ty| {
                    matches!(ty.category(), TypeCategory::Struct | TypeCategory::FixedArray)
                })
            {
                moves.push((block, index, instruction, place, result));
                if moves.len() == 2 {
                    break 'blocks;
                }
            }
        }
    }
    if let Some((_, _, instruction, _, _)) = moves.get(1) {
        errors.push(error_at(
            "ZRYNA-I3010",
            instruction.span,
            "function contains more than one aggregate projection move",
            "move at most one complete static Struct or fixed-array subobject into one exact direct local, final return, or static replacement under a distinct local root",
        ));
        return;
    }
    let Some((block, instruction_index, instruction, place, result)) = moves.first().copied()
    else {
        return;
    };
    let temporary_owner = unique_temporary_owner(function, result.id, result.ty);
    let direct_local = block.instructions.get(instruction_index + 1).is_some_and(|next| {
        matches!(
            next.kind,
            raw::InstructionKind::InitializePlace { place: target, value }
                if value == result.id
                    && function.places.get(target.0 as usize).is_some_and(|target| {
                        target.ty == result.ty && matches!(target.kind, raw::PlaceKind::Local(_))
                    })
        )
    });
    let projected_replacement = block.instructions.get(instruction_index + 1).is_some_and(|next| {
        is_complete_projected_subobject_replacement(next, place, result, temporary_owner, function)
    });
    let direct_return = instruction_index + 1 == block.instructions.len()
        && block.terminators.len() == 1
        && block.terminators.first().is_some_and(|terminator| {
            matches!(
                &terminator.kind,
                raw::Terminator::Return { value, .. }
                    if *value == result.id && function.result == result.ty
            )
        });
    let source_root = root_place(place, function);
    let local_source = function
        .places
        .get(source_root.0 as usize)
        .is_some_and(|source| matches!(source.kind, raw::PlaceKind::Local(_)));
    let exact_consumer = direct_local
        || projected_replacement
        || (direct_return
            && function.parameters.is_empty()
            && function.borrow_parameters.is_empty()
            && local_source);
    let uses = function
        .blocks
        .iter()
        .flat_map(|candidate| {
            candidate
                .instructions
                .iter()
                .flat_map(|candidate| instruction_operands(&candidate.kind))
                .chain(
                    candidate
                        .terminators
                        .iter()
                        .flat_map(|terminator| terminator_operands(&terminator.kind)),
                )
        })
        .filter(|value| *value == result.id)
        .count();
    if !private_straight_line
        || !static_projection_path(place, function)
        || !has_exact_projection_topology(place, function, layouts)
        || temporary_owner.is_none()
        || !exact_consumer
        || uses != 1
    {
        errors.push(error_at(
            "ZRYNA-I3010",
            instruction.span,
            "aggregate projection move escapes its exact direct-local, final-return, or projected-assignment context",
            "move one complete static Struct or fixed-array subobject immediately into one same-type local initialization, final return, or static replacement under a distinct local root",
        ));
    }
}

fn value_use_count(function: &raw::Function, value: raw::ValueId) -> usize {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .instructions
                .iter()
                .flat_map(|instruction| instruction_operands(&instruction.kind))
                .chain(
                    block
                        .terminators
                        .iter()
                        .flat_map(|terminator| terminator_operands(&terminator.kind)),
                )
        })
        .filter(|candidate| *candidate == value)
        .count()
}

fn unique_temporary_owner(
    function: &raw::Function,
    value: raw::ValueId,
    ty: raw::TypeId,
) -> Option<raw::PlaceId> {
    let mut owners = function.places.iter().filter(|candidate| {
        candidate.ty == ty
            && matches!(candidate.kind, raw::PlaceKind::Temporary(owner) if owner == value)
    });
    let owner = owners.next()?.id;
    owners.next().is_none().then_some(owner)
}

fn is_complete_projected_subobject_replacement(
    instruction: &raw::Instruction,
    source: raw::PlaceId,
    result: raw::ValueDefinition,
    temporary_owner: Option<raw::PlaceId>,
    function: &raw::Function,
) -> bool {
    let raw::InstructionKind::ReplacePlace { place: target, value, .. } = instruction.kind else {
        return false;
    };
    let source_root = root_place(source, function);
    let target_root = root_place(target, function);
    value == result.id
        && function.parameters.is_empty()
        && function.borrow_parameters.is_empty()
        && source_root != target_root
        && function
            .places
            .get(source_root.0 as usize)
            .is_some_and(|source_root| matches!(source_root.kind, raw::PlaceKind::Local(_)))
        && function
            .places
            .get(target_root.0 as usize)
            .is_some_and(|target_root| matches!(target_root.kind, raw::PlaceKind::Local(_)))
        && function.places.get(target.0 as usize).is_some_and(|target| target.ty == result.ty)
        && static_projection_path(target, function)
        && temporary_owner.is_some()
}

fn admitted_projected_assignment_source(
    source: raw::PlaceId,
    result: raw::ValueDefinition,
    cloned: bool,
    target: raw::PlaceId,
    temporary_owner: Option<raw::PlaceId>,
    function: &raw::Function,
    layouts: &VerifiedLayouts,
) -> bool {
    let source_root = root_place(source, function);
    let target_root = root_place(target, function);
    let same_type =
        function.places.get(source.0 as usize).is_some_and(|place| place.ty == result.ty)
            && function.places.get(target.0 as usize).is_some_and(|place| place.ty == result.ty);
    let direct_root = source == source_root
        && function
            .places
            .get(source.0 as usize)
            .is_some_and(|place| matches!(place.kind, raw::PlaceKind::Local(_)));
    let complete_projected_move = !cloned
        && source != source_root
        && function.parameters.is_empty()
        && function.borrow_parameters.is_empty()
        && function
            .places
            .get(source_root.0 as usize)
            .is_some_and(|source_root| matches!(source_root.kind, raw::PlaceKind::Local(_)))
        && static_projection_path(source, function)
        && has_exact_projection_topology(source, function, layouts)
        && temporary_owner.is_some();
    let complete_projected_clone = cloned
        && source != source_root
        && function.parameters.is_empty()
        && function.borrow_parameters.is_empty()
        && function
            .places
            .get(source_root.0 as usize)
            .is_some_and(|source_root| matches!(source_root.kind, raw::PlaceKind::Local(_)))
        && static_projection_path(source, function)
        && temporary_owner.is_some();
    source_root != target_root
        && same_type
        && (direct_root || complete_projected_move || complete_projected_clone)
}

fn verify_projected_aggregate_clone_contexts(
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    errors: &mut Errors,
) {
    let private_straight_line = function.entry_export.is_none()
        && function.blocks.len() == 1
        && function
            .blocks
            .first()
            .and_then(|block| block.terminators.first())
            .is_some_and(|terminator| terminator_edges(&terminator.kind).is_empty());
    let mut clones = Vec::with_capacity(2);
    'blocks: for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let raw::InstructionKind::ClonePlace { place, .. } = instruction.kind else {
                continue;
            };
            let Some(result) = instruction.result else { continue };
            let Some(source) = function.places.get(place.0 as usize) else { continue };
            if projection_base(&source.kind).is_some() {
                clones.push((block, index, instruction, place, result));
                if clones.len() == 2 {
                    break 'blocks;
                }
            }
        }
    }
    if let Some((_, _, instruction, _, _)) = clones.get(1) {
        errors.push(error_at(
            "ZRYNA-I3010",
            instruction.span,
            "function contains more than one projected aggregate clone",
            "clone at most one static Struct or fixed-array subobject into one exact direct local or static replacement under a distinct local root",
        ));
        return;
    }
    let Some((block, instruction_index, instruction, place, result)) = clones.first().copied()
    else {
        return;
    };
    let valid_type = layout_type(layouts, result.ty).is_some_and(|ty| {
        ty.drop_kind() != 0
            && matches!(ty.category(), TypeCategory::Struct | TypeCategory::FixedArray)
    });
    let temporary_owner = unique_temporary_owner(function, result.id, result.ty);
    let direct_local = block.instructions.get(instruction_index + 1).is_some_and(|next| {
        matches!(
            next.kind,
            raw::InstructionKind::InitializePlace { place: target, value }
                if value == result.id
                    && function.places.get(target.0 as usize).is_some_and(|target| {
                        target.ty == result.ty && matches!(target.kind, raw::PlaceKind::Local(_))
                    })
        )
    });
    let projected_replacement = block.instructions.get(instruction_index + 1).is_some_and(|next| {
        is_complete_projected_subobject_replacement(next, place, result, temporary_owner, function)
    });
    let uses = value_use_count(function, result.id);
    if !private_straight_line
        || !valid_type
        || !static_projection_path(place, function)
        || temporary_owner.is_none()
        || !(direct_local || projected_replacement)
        || uses != 1
    {
        errors.push(error_at(
            "ZRYNA-I3010",
            instruction.span,
            "projected aggregate clone escapes its exact direct-local or projected-assignment context",
            "clone one initialized static Struct or fixed-array subobject immediately into one same-type local initialization or static replacement under a distinct local root",
        ));
    }
}

fn verify_projected_aggregate_assignment_contexts(
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    errors: &mut Errors,
) {
    let private_straight_line = function.entry_export.is_none()
        && function.blocks.len() == 1
        && function
            .blocks
            .first()
            .and_then(|block| block.terminators.first())
            .is_some_and(|terminator| terminator_edges(&terminator.kind).is_empty());
    let mut replacements = Vec::with_capacity(2);
    'blocks: for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let raw::InstructionKind::ReplacePlace { place, value, .. } = instruction.kind else {
                continue;
            };
            let Some(target) = function.places.get(place.0 as usize) else { continue };
            if projection_base(&target.kind).is_some()
                && layout_type(layouts, target.ty).is_some_and(|ty| {
                    matches!(ty.category(), TypeCategory::Struct | TypeCategory::FixedArray)
                })
            {
                replacements.push((block, index, instruction, place, value));
                if replacements.len() == 2 {
                    break 'blocks;
                }
            }
        }
    }
    if let Some((_, _, instruction, _, _)) = replacements.get(1) {
        errors.push(error_at(
            "ZRYNA-I3010",
            instruction.span,
            "function contains more than one projected aggregate assignment",
            "move or clone at most one complete static aggregate subobject, or move or clone one complete aggregate root, into one static Struct or fixed-array projection",
        ));
        return;
    }
    let Some((block, instruction_index, instruction, target, value)) =
        replacements.first().copied()
    else {
        return;
    };
    let target_root = root_place(target, function);
    let target_is_local = function
        .places
        .get(target_root.0 as usize)
        .is_some_and(|place| matches!(place.kind, raw::PlaceKind::Local(_)));
    let valid_type = function.places.get(target.0 as usize).is_some_and(|place| {
        layout_type(layouts, place.ty).is_some_and(|ty| {
            ty.drop_kind() != 0
                && matches!(ty.category(), TypeCategory::Struct | TypeCategory::FixedArray)
        }) && structural_clone_capable(place.ty, layouts)
    });
    let preceding_source = instruction_index.checked_sub(1).and_then(|index| {
        let preceding = block.instructions.get(index)?;
        let (source, cloned) = match preceding.kind {
            raw::InstructionKind::MoveFromPlace { place: source } => (source, false),
            raw::InstructionKind::ClonePlace { place: source, .. } => (source, true),
            _ => return None,
        };
        let result = preceding.result?;
        (result.id == value).then_some((source, result, cloned))
    });
    let target_ty = function.places.get(target.0 as usize).map(|place| place.ty);
    let temporary_owner = target_ty.and_then(|ty| unique_temporary_owner(function, value, ty));
    let source_is_admitted = preceding_source.is_some_and(|(source, result, cloned)| {
        admitted_projected_assignment_source(
            source,
            result,
            cloned,
            target,
            temporary_owner,
            function,
            layouts,
        )
    });
    let uses = value_use_count(function, value);
    if !private_straight_line
        || !valid_type
        || !target_is_local
        || !static_projection_path(target, function)
        || !source_is_admitted
        || temporary_owner.is_none()
        || uses != 1
    {
        errors.push(error_at(
            "ZRYNA-I3010",
            instruction.span,
            "projected aggregate assignment escapes its exact source-to-static-projection context",
            "move or clone one distinct complete static Struct or fixed-array subobject, or move or clone one distinct complete local root, immediately into one same-type static projection",
        ));
    }
}

fn instruction_place_operands(kind: &raw::InstructionKind) -> Vec<raw::PlaceId> {
    use raw::InstructionKind as I;
    match kind {
        I::CopyFromPlace { place }
        | I::MoveFromPlace { place }
        | I::GenericMoveFromPlace { place }
        | I::ClonePlace { place, .. }
        | I::GenericClonePlace { place, .. }
        | I::HandleAwareClonePlace { place, .. }
        | I::InitializePlace { place, .. }
        | I::ReplacePlace { place, .. }
        | I::GenericReplacePlace { place, .. }
        | I::DropPlace { place }
        | I::EnumDiscriminant { place }
        | I::FixedArrayIndexCopy { place, .. }
        | I::VecIndexCopy { place, .. }
        | I::StringClone { place, .. }
        | I::VecClone { place, .. }
        | I::SharedClone { place, .. }
        | I::WeakDowngrade { place, .. }
        | I::WeakClone { place, .. } => vec![*place],
        I::StringConcat { left, right, .. } => vec![*left, *right],
        I::VecPush { vector, .. } => vec![*vector],
        I::BeginBorrow(definition)
        | I::BeginIndexedBorrow { definition, .. }
        | I::BeginIndexedAccess { definition, .. } => {
            vec![definition.place]
        }
        _ => vec![],
    }
}

fn terminator_place_operands(kind: &raw::Terminator) -> Vec<raw::PlaceId> {
    match kind {
        raw::Terminator::EnumMatch { place, .. } => vec![*place],
        raw::Terminator::WeakUpgradeBranch { weak, .. } => vec![*weak],
        _ => vec![],
    }
}

#[allow(clippy::too_many_lines)]
fn derive_value_owners(
    function: &raw::Function,
    values: &[ValueInfo],
    layouts: &VerifiedLayouts,
    errors: &mut Errors,
) -> Vec<Option<raw::PlaceId>> {
    let is_non_copy = |id: raw::ValueId| {
        value_info(values, id)
            .and_then(|info| layout_type(layouts, info.ty))
            .is_some_and(|ty| ty.drop_kind() != 0)
    };
    let mut owners = vec![None; values.len()];
    for place in &function.places {
        let value = match place.kind {
            raw::PlaceKind::Parameter(parameter) => Some(raw::ValueId(parameter)),
            raw::PlaceKind::Temporary(value) => Some(value),
            _ => None,
        };
        let Some(value) = value else { continue };
        let Some(info) = value_info(values, value) else { continue };
        let root_is_valid = info.ty == place.ty && projection_base(&place.kind).is_none();
        if !root_is_valid {
            errors.push(error_at(
                "ZRYNA-I3008",
                place.span,
                "parameter or temporary place root is ill-typed",
                "bind each addressable root to one existing value of the exact sealed type",
            ));
            continue;
        }
        if !is_non_copy(value) {
            continue;
        }
        let Some(slot) = owners.get_mut(value.0 as usize) else { continue };
        if slot.replace(place.id).is_some() {
            errors.push(error_at(
                "ZRYNA-I3008",
                place.span,
                "non-Copy value has more than one root owner",
                "bind each non-Copy value to exactly one parameter or temporary root",
            ));
        }
    }
    for (index, info) in values.iter().enumerate() {
        let non_copy = layout_type(layouts, info.ty).is_some_and(|ty| ty.drop_kind() != 0);
        if non_copy != owners[index].is_some() {
            errors.push(error_at(
                "ZRYNA-I3008",
                function.span,
                if non_copy {
                    "non-Copy value has no root owner"
                } else {
                    "Copy value unexpectedly has a root owner"
                },
                "derive exactly one owner root for every non-Copy value and none for Copy values",
            ));
        }
    }
    owners
}

fn verify_reducible_loops(
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    dominators: &[BTreeSet<usize>],
    span: Option<Span>,
    errors: &mut Errors,
) {
    let components = strongly_connected_components(successors);
    let mut nesting = vec![0usize; successors.len()];
    for component in components {
        let cyclic = component.len() > 1
            || component.first().is_some_and(|node| successors[*node].contains(node));
        if !cyclic {
            continue;
        }
        let headers = component
            .iter()
            .copied()
            .filter(|candidate| component.iter().all(|node| dominators[*node].contains(candidate)))
            .collect::<Vec<_>>();
        if headers.len() != 1 {
            errors.push(error_with_optional_span(
                "ZRYNA-I3007",
                span,
                "CFG contains an irreducible control-flow cycle",
                "use one dominating loop header for every cycle",
            ));
            return;
        }
        let header = headers[0];
        if component.iter().any(|node| {
            predecessors[*node]
                .iter()
                .any(|predecessor| !component.contains(predecessor) && *node != header)
        }) {
            errors.push(error_with_optional_span(
                "ZRYNA-I3007",
                span,
                "loop has an entry that bypasses its verified header",
                "route every external loop entry through its dominating header",
            ));
            return;
        }
        for source in &component {
            for target in &successors[*source] {
                if component.contains(target) && dominators[*source].contains(target) {
                    let mut loop_nodes = BTreeSet::from([*target, *source]);
                    let mut work = VecDeque::from([*source]);
                    while let Some(node) = work.pop_front() {
                        for predecessor in &predecessors[node] {
                            if loop_nodes.insert(*predecessor) {
                                work.push_back(*predecessor);
                            }
                        }
                    }
                    for node in loop_nodes {
                        nesting[node] = nesting[node].saturating_add(1);
                        if nesting[node] > MAX_LOOP_NESTING {
                            errors.limit("verified loop nesting", MAX_LOOP_NESTING);
                            return;
                        }
                    }
                }
            }
        }
    }
}

fn strongly_connected_components(successors: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut visited = vec![false; successors.len()];
    let mut order = Vec::with_capacity(successors.len());
    for start in 0..successors.len() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((node, edge_index)) = stack.pop() {
            if let Some(target) = successors[node].get(edge_index).copied() {
                stack.push((node, edge_index + 1));
                if !visited[target] {
                    visited[target] = true;
                    stack.push((target, 0));
                }
            } else {
                order.push(node);
            }
        }
    }
    let mut reverse = vec![Vec::new(); successors.len()];
    for (source, edges) in successors.iter().enumerate() {
        for target in edges {
            reverse[*target].push(source);
        }
    }
    visited.fill(false);
    let mut components = Vec::new();
    for start in order.into_iter().rev() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            component.push(node);
            for predecessor in &reverse[node] {
                if !visited[*predecessor] {
                    visited[*predecessor] = true;
                    stack.push(*predecessor);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }
    components
}

fn verify_terminator_types(
    terminator: &raw::SpannedTerminator,
    function: &raw::Function,
    values: &[ValueInfo],
    layouts: &VerifiedLayouts,
    errors: &mut Errors,
) {
    let valid = match &terminator.kind {
        raw::Terminator::Return { value, .. } => {
            value_info(values, *value).is_some_and(|info| info.ty == function.result)
        }
        raw::Terminator::Branch { condition, .. } => value_info(values, *condition)
            .and_then(|info| layout_type(layouts, info.ty))
            .is_some_and(|ty| ty.category() == TypeCategory::Bool),
        raw::Terminator::EnumMatch { place, arms } => function
            .places
            .get(place.0 as usize)
            .and_then(|place| layout_type(layouts, place.ty))
            .is_some_and(|ty| {
                ty.category() == TypeCategory::Enum
                    && arms.len() == ty.variants().len()
                    && arms
                        .iter()
                        .zip(ty.variants())
                        .all(|(arm, variant)| arm.variant == variant.ordinal())
            }),
        raw::Terminator::WeakUpgradeBranch { weak, success, expired, .. } => function
            .places
            .get(weak.0 as usize)
            .and_then(|place| layout_type(layouts, place.ty))
            .is_some_and(|weak_ty| {
                let Some(payload) = weak_ty.referenced_type() else { return false };
                if weak_ty.category() != TypeCategory::Weak {
                    return false;
                }
                let Some(success_block) = function.blocks.get(success.target.0 as usize) else {
                    return false;
                };
                let Some(first) = success_block.parameters.first() else { return false };
                layout_type(layouts, first.ty).is_some_and(|shared| {
                    shared.category() == TypeCategory::Shared
                        && shared.referenced_type() == Some(payload)
                        && success.arguments.len() + 1 == success_block.parameters.len()
                }) && function
                    .blocks
                    .get(expired.target.0 as usize)
                    .is_some_and(|block| expired.arguments.len() == block.parameters.len())
            }),
        raw::Terminator::Jump(_) | raw::Terminator::Trap { .. } => true,
    };
    if !valid {
        errors.push(error_at(
            "ZRYNA-I3014",
            terminator.span,
            "terminator result, condition, enum arms, or weak-upgrade edge shape is invalid",
            "use exact return/bool/exhaustive-enum/weak-upgrade types and successor shapes",
        ));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaceStateKind {
    Uninitialized,
    Initialized,
    PartiallyInitialized,
    PartiallyMoved,
    Moved,
    Dropped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlaceState {
    kind: PlaceStateKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OwnershipFlow {
    states: Vec<PlaceState>,
    variants: Vec<Option<u32>>,
    pending: Vec<raw::PlaceId>,
}

#[allow(clippy::too_many_lines)]
fn sealed_drop_actions(
    owner: FunctionIdentity,
    function: &raw::Function,
    plan: raw::CleanupPlanId,
    states: &[PlaceState],
    variants: &[Option<u32>],
) -> Vec<VerifiedDropAction> {
    function.cleanup_plans[plan.0 as usize]
        .actions
        .iter()
        .map(|action| {
            let (root, kind) = match action {
                raw::DropAction::DropPlace(root) => (*root, VerifiedDropActionKind::Place),
                raw::DropAction::DropVecInitializedPrefix(root) => {
                    (*root, VerifiedDropActionKind::VecInitializedPrefix)
                }
                raw::DropAction::DropAggregateInitializedPrefix(root) => {
                    (*root, VerifiedDropActionKind::AggregateInitializedPrefix)
                }
                raw::DropAction::DropGenericCloneInitializedPrefix(root) => {
                    (*root, VerifiedDropActionKind::GenericCloneInitializedPrefix)
                }
            };
            sealed_drop_action_with_kind(owner, function, root, states, variants, kind)
        })
        .collect()
}

fn sealed_drop_action(
    owner: FunctionIdentity,
    function: &raw::Function,
    root: raw::PlaceId,
    states: &[PlaceState],
    variants: &[Option<u32>],
) -> VerifiedDropAction {
    sealed_drop_action_with_kind(
        owner,
        function,
        root,
        states,
        variants,
        VerifiedDropActionKind::Place,
    )
}

fn sealed_drop_action_with_kind(
    owner: FunctionIdentity,
    function: &raw::Function,
    root: raw::PlaceId,
    states: &[PlaceState],
    variants: &[Option<u32>],
    kind: VerifiedDropActionKind,
) -> VerifiedDropAction {
    let moved_projections = function
        .places
        .iter()
        .filter(|place| {
            is_projection_below(place.id, root, &function.places)
                && matches!(
                    states[place.id.0 as usize].kind,
                    PlaceStateKind::Moved | PlaceStateKind::Dropped
                )
        })
        .map(|place| PlaceIdentity { owner, index: place.id.0 })
        .collect();
    let initialized_projections = function
        .places
        .iter()
        .filter(|place| {
            is_projection_below(place.id, root, &function.places)
                && states[place.id.0 as usize].kind == PlaceStateKind::Initialized
        })
        .map(|place| PlaceIdentity { owner, index: place.id.0 })
        .collect();
    let active_variants = function
        .places
        .iter()
        .filter(|place| place.id == root || is_projection_below(place.id, root, &function.places))
        .filter_map(|place| {
            variants[place.id.0 as usize].map(|variant| VerifiedActiveVariant {
                place: PlaceIdentity { owner, index: place.id.0 },
                variant,
            })
        })
        .collect();
    VerifiedDropAction {
        root: PlaceIdentity { owner, index: root.0 },
        kind,
        moved_projections,
        initialized_projections,
        active_variant: variants[root.0 as usize],
        active_variants,
    }
}

fn is_projection_below(mut place: raw::PlaceId, root: raw::PlaceId, places: &[raw::Place]) -> bool {
    let mut visited = BTreeSet::new();
    while visited.insert(place) {
        let Some(base) =
            places.get(place.0 as usize).and_then(|place| projection_base(&place.kind))
        else {
            return false;
        };
        if base == root {
            return true;
        }
        place = base;
    }
    false
}

#[allow(clippy::too_many_lines)]
fn verify_ownership_dataflow(
    function: &raw::Function,
    value_owners: &[Option<raw::PlaceId>],
    layouts: &VerifiedLayouts,
    successors: &[Vec<usize>],
    borrows: &BorrowIndex,
    errors: &mut Errors,
) {
    if function.blocks.is_empty() {
        return;
    }
    let mut initial = function
        .places
        .iter()
        .map(|place| PlaceState {
            kind: if matches!(place.kind, raw::PlaceKind::Parameter(_)) {
                PlaceStateKind::Initialized
            } else {
                PlaceStateKind::Uninitialized
            },
        })
        .collect::<Vec<_>>();
    for index in 0..function.places.len() {
        if let Some(base) = projection_base(&function.places[index].kind)
            && initial
                .get(base.0 as usize)
                .is_some_and(|state| state.kind == PlaceStateKind::Initialized)
        {
            initial[index] = PlaceState { kind: PlaceStateKind::Initialized };
        }
    }
    let pending = function
        .parameters
        .iter()
        .filter_map(|parameter| value_owners.get(parameter.id.0 as usize).copied().flatten())
        .collect();
    let mut entries = vec![None::<OwnershipFlow>; function.blocks.len()];
    entries[0] = Some(OwnershipFlow {
        states: initial,
        variants: vec![None; function.places.len()],
        pending,
    });
    let mut queue = BTreeSet::from([0usize]);
    while let Some(block_index) = queue.pop_first() {
        let Some(mut flow) = entries[block_index].clone() else {
            continue;
        };
        let mut active = borrows.entry_active(function, block_index);
        let block = &function.blocks[block_index];
        for instruction in &block.instructions {
            indexed_borrows::verify_consumption(instruction, function, layouts, &active, errors);
            let call = matches!(instruction.kind, raw::InstructionKind::DirectCall { .. });
            if call {
                indexed_borrows::invalidate_call_variants(
                    instruction,
                    function,
                    borrows,
                    &mut flow.variants,
                );
                transfer_consumed_values(
                    &consuming_instruction_operands(&instruction.kind),
                    value_owners,
                    function,
                    &mut flow,
                    instruction.span,
                    errors,
                );
            }
            if let Some(cleanup) = instruction_cleanup(&instruction.kind) {
                verify_cleanup(cleanup, function, layouts, &flow, errors);
            }
            if let raw::InstructionKind::VecClone { element_cleanup: Some(cleanup), .. } =
                &instruction.kind
            {
                let result_owner = instruction
                    .result
                    .and_then(|result| value_owners.get(result.id.0 as usize).copied().flatten());
                verify_vec_clone_element_cleanup(*cleanup, result_owner, function, &flow, errors);
            }
            if let raw::InstructionKind::ClonePlace {
                place, element_cleanup: Some(cleanup), ..
            } = &instruction.kind
            {
                let result_owner = instruction
                    .result
                    .and_then(|result| value_owners.get(result.id.0 as usize).copied().flatten());
                verify_aggregate_clone_element_cleanup(
                    *cleanup,
                    result_owner,
                    *place,
                    function,
                    layouts,
                    &flow,
                    errors,
                );
            }
            generic_clone::verify_prefix_cleanup(
                instruction,
                value_owners,
                function,
                borrows,
                &flow,
                errors,
            );
            handle_aware_clone::verify_prefix_cleanup(
                instruction,
                value_owners,
                function,
                borrows,
                &flow,
                errors,
            );
            apply_ownership_instruction(
                instruction,
                function,
                (layouts, borrows),
                &mut flow.states,
                &mut flow.variants,
                &mut active,
                errors,
            );
            if !call {
                apply_value_transfers(
                    instruction,
                    value_owners,
                    function,
                    layouts,
                    &mut flow,
                    errors,
                );
            }
            if let Some(result) = instruction.result
                && let Some(owner) = value_owners.get(result.id.0 as usize).copied().flatten()
            {
                if !matches!(
                    instruction.kind,
                    raw::InstructionKind::MoveFromPlace { .. }
                        | raw::InstructionKind::GenericMoveFromPlace { .. }
                ) {
                    push_pending_owner(owner, function, &mut flow, instruction.span, errors);
                }
                match instruction.kind {
                    raw::InstructionKind::EnumConstruct { variant, .. } => {
                        flow.variants[owner.0 as usize] = Some(variant);
                    }
                    raw::InstructionKind::ClonePlace { place, .. }
                    | raw::InstructionKind::GenericClonePlace { place, .. }
                    | raw::InstructionKind::HandleAwareClonePlace { place, .. } => {
                        flow.variants[owner.0 as usize] = flow.variants[place.0 as usize];
                    }
                    raw::InstructionKind::MoveFromPlace { .. }
                    | raw::InstructionKind::GenericMoveFromPlace { .. } => {}
                    _ => flow.variants[owner.0 as usize] = None,
                }
            }
        }
        if let Some(terminator) = block.terminators.first() {
            transient_edges::verify_completion(function, borrows, &active, terminator, errors);
            let place_read = match terminator.kind {
                raw::Terminator::EnumMatch { place, .. }
                | raw::Terminator::WeakUpgradeBranch { weak: place, .. } => Some(place),
                _ => None,
            };
            if place_read.is_some_and(|place| {
                flow.states
                    .get(place.0 as usize)
                    .is_none_or(|state| state.kind != PlaceStateKind::Initialized)
                    || overlaps_exclusive(place, &active, &function.places)
            }) {
                errors.push(error_at(
                    "ZRYNA-I3010",
                    terminator.span,
                    "enum or weak terminator reads an unavailable or exclusively borrowed owner",
                    "branch only on one initialized owner not hidden by an exclusive borrow",
                ));
            }
            match &terminator.kind {
                raw::Terminator::Return { value, cleanup } => {
                    transfer_return_value(
                        *value,
                        value_owners,
                        function,
                        layouts,
                        &mut flow,
                        terminator.span,
                        errors,
                    );
                    verify_cleanup(*cleanup, function, layouts, &flow, errors);
                }
                raw::Terminator::Trap { cleanup, .. }
                | raw::Terminator::WeakUpgradeBranch { cleanup, .. } => {
                    verify_cleanup(*cleanup, function, layouts, &flow, errors);
                }
                _ => {}
            }
        }
        for (edge_index, successor) in successors[block_index].iter().enumerate() {
            let mut incoming = flow.clone();
            if let Some(raw::SpannedTerminator {
                kind: raw::Terminator::EnumMatch { place, arms },
                ..
            }) = block.terminators.first()
                && let Some(arm) = arms.get(edge_index)
            {
                incoming.variants[place.0 as usize] = Some(arm.variant);
            }
            if let Some(terminator) = block.terminators.first()
                && let Some(edge) = terminator_edges(&terminator.kind).get(edge_index)
            {
                transient_edges::verify_edge_consumption(
                    edge,
                    value_owners,
                    function,
                    &active,
                    terminator.span,
                    errors,
                );
                transfer_edge_owners(
                    edge,
                    &terminator.kind,
                    function,
                    value_owners,
                    &mut incoming,
                    terminator.span,
                    errors,
                );
            }
            normalize_dead_places(&mut incoming, function, layouts);
            match &entries[*successor] {
                None => {
                    entries[*successor] = Some(incoming);
                    queue.insert(*successor);
                }
                Some(existing) if existing != &incoming => errors.push(error_at("ZRYNA-I3010", block.terminators[0].span, "ownership, initialization, or active-enum state differs across a CFG join or backedge", "restore every live place and enum refinement to one exact state on every incoming edge")),
                Some(_) => {}
            }
        }
    }
}

fn root_place(mut place: raw::PlaceId, function: &raw::Function) -> raw::PlaceId {
    let mut seen = BTreeSet::new();
    while seen.insert(place) {
        let Some(base) =
            function.places.get(place.0 as usize).and_then(|place| projection_base(&place.kind))
        else {
            break;
        };
        place = base;
    }
    place
}

fn pending_slot(flow: &OwnershipFlow, owner: raw::PlaceId) -> Option<usize> {
    flow.pending.iter().position(|candidate| *candidate == owner)
}

fn remove_pending_owner(
    owner: raw::PlaceId,
    function: &raw::Function,
    flow: &mut OwnershipFlow,
    span: Span,
    errors: &mut Errors,
) {
    let Some(slot) = pending_slot(flow, owner) else {
        ownership_error(span, "non-Copy value owner is unavailable or already consumed", errors);
        return;
    };
    flow.pending.remove(slot);
    flow.states[owner.0 as usize] = PlaceState { kind: PlaceStateKind::Moved };
    flow.variants[owner.0 as usize] = None;
    for place in &function.places {
        if is_projection_below(place.id, owner, &function.places) {
            flow.states[place.id.0 as usize] = PlaceState { kind: PlaceStateKind::Moved };
            flow.variants[place.id.0 as usize] = None;
        }
    }
}

fn push_pending_owner(
    owner: raw::PlaceId,
    function: &raw::Function,
    flow: &mut OwnershipFlow,
    span: Span,
    errors: &mut Errors,
) {
    if pending_slot(flow, owner).is_some() {
        ownership_error(span, "non-Copy result owner is already live", errors);
        return;
    }
    flow.states[owner.0 as usize] = PlaceState { kind: PlaceStateKind::Initialized };
    initialize_projections(owner, function, &mut flow.states);
    flow.pending.push(owner);
}

fn rename_pending_owner(
    source: raw::PlaceId,
    target: raw::PlaceId,
    function: &raw::Function,
    flow: &mut OwnershipFlow,
    span: Span,
    errors: &mut Errors,
) {
    let Some(slot) = pending_slot(flow, source) else {
        ownership_error(span, "ownership rename source is unavailable", errors);
        return;
    };
    if source != target && pending_slot(flow, target).is_some() {
        ownership_error(span, "ownership rename target is already live", errors);
        return;
    }
    if source == target {
        return;
    }
    let source_kind = flow.states[source.0 as usize].kind;
    let source_variant = flow.variants[source.0 as usize];
    let source_projections = function
        .places
        .iter()
        .filter_map(|place| {
            projection_path(place.id, source, &function.places).map(|path| {
                (path, flow.states[place.id.0 as usize], flow.variants[place.id.0 as usize])
            })
        })
        .collect::<Vec<_>>();
    if source_kind != PlaceStateKind::Initialized {
        let source_paths =
            source_projections.iter().map(|(path, _, _)| path.clone()).collect::<BTreeSet<_>>();
        let target_paths = function
            .places
            .iter()
            .filter_map(|place| projection_path(place.id, target, &function.places))
            .collect::<BTreeSet<_>>();
        if source_paths != target_paths {
            ownership_error(
                span,
                "partial owner rename requires exact matching projection metadata",
                errors,
            );
            return;
        }
    }
    flow.pending[slot] = target;
    flow.states[source.0 as usize] = PlaceState { kind: PlaceStateKind::Moved };
    flow.variants[source.0 as usize] = None;
    for place in &function.places {
        if is_projection_below(place.id, source, &function.places) {
            flow.states[place.id.0 as usize] = PlaceState { kind: PlaceStateKind::Moved };
            flow.variants[place.id.0 as usize] = None;
        }
    }
    flow.states[target.0 as usize] = PlaceState { kind: source_kind };
    flow.variants[target.0 as usize] = source_variant;
    for place in &function.places {
        let Some(path) = projection_path(place.id, target, &function.places) else { continue };
        if let Some((_, state, variant)) =
            source_projections.iter().find(|(source_path, _, _)| *source_path == path)
        {
            flow.states[place.id.0 as usize] = *state;
            flow.variants[place.id.0 as usize] = *variant;
        } else if source_kind == PlaceStateKind::Initialized {
            flow.states[place.id.0 as usize] = PlaceState { kind: PlaceStateKind::Initialized };
            flow.variants[place.id.0 as usize] = None;
        } else {
            ownership_error(
                span,
                "partial owner rename lacks matching projection metadata",
                errors,
            );
        }
    }
}

fn consume_owner_into_projection(
    source: raw::PlaceId,
    target: raw::PlaceId,
    target_root: raw::PlaceId,
    function: &raw::Function,
    flow: &mut OwnershipFlow,
    span: Span,
    errors: &mut Errors,
) {
    if places_overlap(source, target_root, &function.places) {
        ownership_error(span, "projection transfer source overlaps its destination owner", errors);
        return;
    }
    let Some(slot) = pending_slot(flow, source) else {
        ownership_error(span, "projection transfer source is unavailable", errors);
        return;
    };
    let source_kind = flow.states[source.0 as usize].kind;
    let source_variant = flow.variants[source.0 as usize];
    let source_projections = function
        .places
        .iter()
        .filter_map(|place| {
            projection_path(place.id, source, &function.places).map(|path| {
                (path, flow.states[place.id.0 as usize], flow.variants[place.id.0 as usize])
            })
        })
        .collect::<Vec<_>>();

    if pending_slot(flow, target_root).is_none() {
        flow.pending[slot] = target_root;
    } else {
        flow.pending.remove(slot);
    }
    flow.states[source.0 as usize] = PlaceState { kind: PlaceStateKind::Moved };
    flow.variants[source.0 as usize] = None;
    for place in &function.places {
        if is_projection_below(place.id, source, &function.places) {
            flow.states[place.id.0 as usize] = PlaceState { kind: PlaceStateKind::Moved };
            flow.variants[place.id.0 as usize] = None;
        }
    }

    flow.states[target.0 as usize] = PlaceState { kind: source_kind };
    flow.variants[target.0 as usize] = source_variant;
    for place in &function.places {
        let Some(path) = projection_path(place.id, target, &function.places) else { continue };
        if let Some((_, state, variant)) =
            source_projections.iter().find(|(source_path, _, _)| *source_path == path)
        {
            flow.states[place.id.0 as usize] = *state;
            flow.variants[place.id.0 as usize] = *variant;
        } else if source_kind == PlaceStateKind::Initialized {
            flow.states[place.id.0 as usize] = PlaceState { kind: PlaceStateKind::Initialized };
            flow.variants[place.id.0 as usize] = None;
        } else {
            ownership_error(span, "projection transfer lacks matching projection metadata", errors);
        }
    }
}

fn projection_path(
    mut place: raw::PlaceId,
    root: raw::PlaceId,
    places: &[raw::Place],
) -> Option<Vec<(u8, u32)>> {
    if place == root {
        return None;
    }
    let mut path = vec![];
    let mut seen = BTreeSet::new();
    while seen.insert(place) {
        let item = places.get(place.0 as usize)?;
        let (base, step) = match item.kind {
            raw::PlaceKind::StructField { base, ordinal } => (base, (0, ordinal)),
            raw::PlaceKind::EnumPayload { base, variant } => (base, (1, variant)),
            raw::PlaceKind::FixedArrayConstant { base, index } => (base, (2, index)),
            _ => return None,
        };
        path.push(step);
        if base == root {
            path.reverse();
            return Some(path);
        }
        place = base;
    }
    None
}

fn transfer_consumed_values(
    values: &[raw::ValueId],
    value_owners: &[Option<raw::PlaceId>],
    function: &raw::Function,
    flow: &mut OwnershipFlow,
    span: Span,
    errors: &mut Errors,
) {
    for value in values {
        if let Some(owner) = value_owners.get(value.0 as usize).copied().flatten() {
            if flow.states[owner.0 as usize].kind != PlaceStateKind::Initialized {
                ownership_error(
                    span,
                    "partial non-Copy owner cannot enter a context without mask transfer",
                    errors,
                );
                continue;
            }
            remove_pending_owner(owner, function, flow, span, errors);
        }
    }
}

fn transfer_return_value(
    value: raw::ValueId,
    value_owners: &[Option<raw::PlaceId>],
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    flow: &mut OwnershipFlow,
    span: Span,
    errors: &mut Errors,
) {
    let Some(owner) = value_owners.get(value.0 as usize).copied().flatten() else { return };
    if flow.states[owner.0 as usize].kind == PlaceStateKind::Initialized {
        remove_pending_owner(owner, function, flow, span, errors);
        return;
    }
    let partial = matches!(
        flow.states[owner.0 as usize].kind,
        PlaceStateKind::PartiallyInitialized | PlaceStateKind::PartiallyMoved
    );
    let temporary = function.places.get(owner.0 as usize).is_some_and(
        |place| matches!(place.kind, raw::PlaceKind::Temporary(candidate) if candidate == value),
    );
    if partial && temporary && has_exact_projection_topology(owner, function, layouts) {
        remove_pending_owner(owner, function, flow, span, errors);
    } else if partial && temporary {
        ownership_error(
            span,
            "partial return owner lacks exact sealed projection topology",
            errors,
        );
    } else {
        ownership_error(
            span,
            "partial non-Copy owner cannot enter a context without mask transfer",
            errors,
        );
    }
}

fn has_exact_projection_topology(
    root: raw::PlaceId,
    function: &raw::Function,
    layouts: &VerifiedLayouts,
) -> bool {
    fn collect(
        ty: zryna_layout::TypeId,
        layouts: &VerifiedLayouts,
        path: &mut Vec<(u8, u32)>,
        active: &mut BTreeSet<u32>,
        expected: &mut BTreeSet<Vec<(u8, u32)>>,
    ) -> bool {
        if !active.insert(ty.index()) {
            return false;
        }
        let Some(record) = layouts.type_by_id(ty) else { return false };
        let valid = match record.category() {
            TypeCategory::Struct => record.fields().iter().all(|field| {
                path.push((0, field.ordinal()));
                expected.insert(path.clone());
                let valid = collect(field.ty(), layouts, path, active, expected);
                path.pop();
                valid
            }),
            TypeCategory::FixedArray => record.referenced_type().is_some_and(|element| {
                let Some(length) =
                    record.array_length().and_then(|length| u32::try_from(length).ok())
                else {
                    return false;
                };
                (0..length).all(|index| {
                    path.push((2, index));
                    expected.insert(path.clone());
                    let valid = collect(element, layouts, path, active, expected);
                    path.pop();
                    valid
                })
            }),
            TypeCategory::Bool | TypeCategory::I32 | TypeCategory::String => true,
            TypeCategory::Enum | TypeCategory::Vec | TypeCategory::Shared | TypeCategory::Weak => {
                false
            }
        };
        active.remove(&ty.index());
        valid
    }

    let Some(ty) = function
        .places
        .get(root.0 as usize)
        .and_then(|place| layout_type(layouts, place.ty))
        .map(zryna_layout::VerifiedType::id)
    else {
        return false;
    };
    if !layouts.type_by_id(ty).is_some_and(|record| {
        matches!(record.category(), TypeCategory::Struct | TypeCategory::FixedArray)
    }) {
        return false;
    }
    let mut expected = BTreeSet::new();
    if !collect(ty, layouts, &mut Vec::new(), &mut BTreeSet::new(), &mut expected) {
        return false;
    }
    let actual = function
        .places
        .iter()
        .filter_map(|place| projection_path(place.id, root, &function.places))
        .collect::<BTreeSet<_>>();
    actual == expected
}

#[allow(clippy::too_many_lines)]
fn apply_value_transfers(
    instruction: &raw::Instruction,
    value_owners: &[Option<raw::PlaceId>],
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    flow: &mut OwnershipFlow,
    errors: &mut Errors,
) {
    use raw::InstructionKind as I;
    match &instruction.kind {
        I::GenericMoveFromPlace { place } => {
            if let Some(result_owner) = instruction
                .result
                .and_then(|result| value_owners.get(result.id.0 as usize).copied().flatten())
            {
                generic_static_places::transfer_complete(
                    *place,
                    result_owner,
                    function,
                    flow,
                    instruction.span,
                    errors,
                );
            }
        }
        I::MoveFromPlace { place } if !place_is_copy(*place, function, layouts) => {
            let Some(result_owner) = instruction
                .result
                .and_then(|result| value_owners.get(result.id.0 as usize).copied().flatten())
            else {
                return;
            };
            let root = root_place(*place, function);
            if root == *place {
                rename_pending_owner(root, result_owner, function, flow, instruction.span, errors);
            } else {
                flow.states[place.0 as usize] = PlaceState { kind: PlaceStateKind::Moved };
                let aggregate = function
                    .places
                    .get(place.0 as usize)
                    .and_then(|place| layout_type(layouts, place.ty))
                    .is_some_and(|ty| {
                        matches!(ty.category(), TypeCategory::Struct | TypeCategory::FixedArray)
                    });
                if aggregate {
                    for projection in &function.places {
                        if is_projection_below(projection.id, *place, &function.places) {
                            flow.states[projection.id.0 as usize] =
                                PlaceState { kind: PlaceStateKind::Moved };
                            flow.variants[projection.id.0 as usize] = None;
                        }
                    }
                }
                mark_ancestors_partial(*place, function, &mut flow.states);
                push_pending_owner(result_owner, function, flow, instruction.span, errors);
            }
        }
        I::InitializePlace { place, value } => {
            let target = root_place(*place, function);
            if let Some(source) = value_owners.get(value.0 as usize).copied().flatten() {
                if target == *place {
                    rename_pending_owner(source, target, function, flow, instruction.span, errors);
                } else {
                    if flow.states[source.0 as usize].kind != PlaceStateKind::Initialized {
                        ownership_error(
                            instruction.span,
                            "partial owner cannot initialize a projection without exact mask transfer",
                            errors,
                        );
                        return;
                    }
                    consume_owner_into_projection(
                        source,
                        *place,
                        target,
                        function,
                        flow,
                        instruction.span,
                        errors,
                    );
                }
            } else if target != *place
                && !place_is_copy(target, function, layouts)
                && pending_slot(flow, target).is_none()
            {
                flow.pending.push(target);
            }
        }
        I::ReplacePlace { place, value, .. } | I::GenericReplacePlace { place, value } => {
            let target = root_place(*place, function);
            let Some(source) = value_owners.get(value.0 as usize).copied().flatten() else {
                return;
            };
            if target != *place {
                if flow.states[place.0 as usize].kind != PlaceStateKind::Initialized {
                    ownership_error(
                        instruction.span,
                        "projection replacement target is unavailable",
                        errors,
                    );
                    return;
                }
                if flow.states[source.0 as usize].kind != PlaceStateKind::Initialized {
                    ownership_error(
                        instruction.span,
                        "partial owner cannot replace a projection without exact mask transfer",
                        errors,
                    );
                    return;
                }
                if pending_slot(flow, target).is_none() {
                    ownership_error(
                        instruction.span,
                        "projection replacement root is not pending",
                        errors,
                    );
                    return;
                }
                consume_owner_into_projection(
                    source,
                    *place,
                    target,
                    function,
                    flow,
                    instruction.span,
                    errors,
                );
                return;
            }
            let source_state = flow.states[source.0 as usize].kind;
            if matches!(
                source_state,
                PlaceStateKind::PartiallyInitialized | PlaceStateKind::PartiallyMoved
            ) && !has_exact_projection_topology(source, function, layouts)
            {
                ownership_error(
                    instruction.span,
                    "partial replacement owner lacks exact sealed projection topology",
                    errors,
                );
                return;
            }
            let Some(target_slot) = pending_slot(flow, target) else {
                ownership_error(instruction.span, "replacement target is not pending", errors);
                return;
            };
            flow.pending.remove(target_slot);
            rename_pending_owner(source, target, function, flow, instruction.span, errors);
        }
        I::VecPush { value, .. } => {
            transfer_consumed_values(
                &[*value],
                value_owners,
                function,
                flow,
                instruction.span,
                errors,
            );
        }
        I::DropPlace { place } => {
            let root = root_place(*place, function);
            if root == *place {
                if let Some(slot) = pending_slot(flow, root) {
                    flow.pending.remove(slot);
                } else {
                    ownership_error(instruction.span, "dropped owner is not pending", errors);
                }
            }
        }
        _ => transfer_consumed_values(
            &consuming_instruction_operands(&instruction.kind),
            value_owners,
            function,
            flow,
            instruction.span,
            errors,
        ),
    }
}

fn transfer_edge_owners(
    edge: &raw::Edge,
    terminator: &raw::Terminator,
    function: &raw::Function,
    value_owners: &[Option<raw::PlaceId>],
    flow: &mut OwnershipFlow,
    span: Span,
    errors: &mut Errors,
) {
    let Some(block) = function.blocks.get(edge.target.0 as usize) else { return };
    let synthesized = matches!(terminator, raw::Terminator::WeakUpgradeBranch { success, .. } if std::ptr::eq(edge, success));
    if synthesized
        && let Some(parameter) = block.parameters.first()
        && let Some(owner) = value_owners.get(parameter.id.0 as usize).copied().flatten()
    {
        push_pending_owner(owner, function, flow, span, errors);
    }
    let parameters = if synthesized && !block.parameters.is_empty() {
        &block.parameters[1..]
    } else {
        &block.parameters[..]
    };
    for (argument, parameter) in edge.arguments.iter().zip(parameters) {
        let source = value_owners.get(argument.0 as usize).copied().flatten();
        let target = value_owners.get(parameter.id.0 as usize).copied().flatten();
        match (source, target) {
            (Some(source), Some(target)) => {
                if flow.states[source.0 as usize].kind != PlaceStateKind::Initialized {
                    ownership_error(
                        span,
                        "partial non-Copy owner cannot enter a CFG edge without mask transfer",
                        errors,
                    );
                    continue;
                }
                rename_pending_owner(source, target, function, flow, span, errors);
            }
            (None, None) => {}
            _ => ownership_error(span, "CFG ownership argument and parameter disagree", errors),
        }
    }
}

fn normalize_dead_places(
    flow: &mut OwnershipFlow,
    function: &raw::Function,
    layouts: &VerifiedLayouts,
) {
    for place in &function.places {
        if projection_base(&place.kind).is_none()
            && layout_type(layouts, place.ty).is_some_and(|ty| ty.drop_kind() != 0)
            && !flow.pending.contains(&place.id)
        {
            flow.states[place.id.0 as usize] = PlaceState { kind: PlaceStateKind::Uninitialized };
            flow.variants[place.id.0 as usize] = None;
            for projection in &function.places {
                if is_projection_below(projection.id, place.id, &function.places) {
                    flow.states[projection.id.0 as usize] =
                        PlaceState { kind: PlaceStateKind::Uninitialized };
                    flow.variants[projection.id.0 as usize] = None;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn apply_ownership_instruction(
    instruction: &raw::Instruction,
    function: &raw::Function,
    authorities: (&VerifiedLayouts, &BorrowIndex),
    states: &mut [PlaceState],
    active_variants: &mut [Option<u32>],
    active: &mut Vec<Option<(raw::PlaceId, raw::BorrowAccess)>>,
    errors: &mut Errors,
) {
    use raw::InstructionKind as I;
    let (layouts, borrows) = authorities;
    let state = |place: raw::PlaceId, states: &[PlaceState]| states.get(place.0 as usize).copied();
    if !matches!(instruction.kind, I::InitializePlace { .. }) {
        for place in instruction_place_operands(&instruction.kind) {
            for (base, variant) in enum_payload_ancestors(place, function) {
                if active_variants.get(base.0 as usize).copied().flatten() != Some(variant) {
                    errors.push(error_at(
                        "ZRYNA-I3013",
                        instruction.span,
                        "enum payload ownership operation does not match the active variant",
                        "operate on a payload only in its exact refined enum arm",
                    ));
                }
            }
        }
    }
    match &instruction.kind {
        I::CopyFromPlace { place } => {
            if state(*place, states).is_none_or(|state| state.kind != PlaceStateKind::Initialized)
                || !place_is_copy(*place, function, layouts)
                || overlaps_exclusive(*place, active, &function.places)
            {
                ownership_error(
                    instruction.span,
                    "copy from a non-Copy or non-initialized place",
                    errors,
                );
            }
        }
        I::MoveFromPlace { place } | I::GenericMoveFromPlace { place } => {
            let whole_non_copy =
                root_place(*place, function) == *place && !place_is_copy(*place, function, layouts);
            let unavailable = state(*place, states).is_none_or(|state| {
                if whole_non_copy {
                    !matches!(
                        state.kind,
                        PlaceStateKind::Initialized
                            | PlaceStateKind::PartiallyInitialized
                            | PlaceStateKind::PartiallyMoved
                    )
                } else {
                    state.kind != PlaceStateKind::Initialized
                }
            }) || overlaps_active(*place, active, &function.places);
            if unavailable {
                ownership_error(
                    instruction.span,
                    "move from an unavailable or borrowed place",
                    errors,
                );
            }
        }
        I::ClonePlace { place, .. }
        | I::GenericClonePlace { place, .. }
        | I::HandleAwareClonePlace { place, .. }
        | I::EnumDiscriminant { place }
        | I::FixedArrayIndexCopy { place, .. }
        | I::VecIndexCopy { place, .. }
        | I::StringClone { place, .. }
        | I::VecClone { place, .. }
        | I::SharedClone { place, .. }
        | I::WeakDowngrade { place, .. }
        | I::WeakClone { place, .. } => {
            if state(*place, states).is_none_or(|state| state.kind != PlaceStateKind::Initialized)
                || overlaps_exclusive(*place, active, &function.places)
            {
                ownership_error(instruction.span, "operation reads an unavailable place", errors);
            }
        }
        I::StringConcat { left, right, .. } => {
            if [*left, *right].into_iter().any(|place| {
                state(place, states).is_none_or(|state| state.kind != PlaceStateKind::Initialized)
                    || overlaps_exclusive(place, active, &function.places)
            }) {
                ownership_error(
                    instruction.span,
                    "string concatenation reads an unavailable place",
                    errors,
                );
            }
        }
        I::VecPush { vector, .. } => {
            if state(*vector, states).is_none_or(|state| state.kind != PlaceStateKind::Initialized)
                || overlaps_active(*vector, active, &function.places)
            {
                ownership_error(
                    instruction.span,
                    "vector push mutates an unavailable or borrowed place",
                    errors,
                );
            }
        }
        I::InitializePlace { place, .. } => {
            let index = place.0 as usize;
            let Some(slot) = states.get(index).copied() else { return };
            if slot.kind != PlaceStateKind::Uninitialized
                || overlaps_active(*place, active, &function.places)
            {
                ownership_error(
                    instruction.span,
                    "initialization targets an initialized or borrowed place",
                    errors,
                );
            } else if validate_projection_initialization(
                *place,
                function,
                layouts,
                states,
                active_variants,
                instruction.span,
                errors,
            ) {
                states[index] = PlaceState { kind: PlaceStateKind::Initialized };
                initialize_projections(*place, function, states);
                active_variants[place.0 as usize] = None;
                promote_initialized_ancestors(*place, function, layouts, states, active_variants);
            }
        }
        I::ReplacePlace { place, .. } | I::GenericReplacePlace { place, .. } => {
            let unavailable = state(*place, states)
                .is_none_or(|state| state.kind != PlaceStateKind::Initialized)
                || overlaps_active(*place, active, &function.places);
            if unavailable {
                ownership_error(
                    instruction.span,
                    "replacement targets an unavailable or borrowed place",
                    errors,
                );
            } else {
                active_variants[place.0 as usize] = None;
            }
        }
        I::DropPlace { place } => {
            let unavailable = state(*place, states).is_none_or(|state| {
                !matches!(
                    state.kind,
                    PlaceStateKind::Initialized
                        | PlaceStateKind::PartiallyInitialized
                        | PlaceStateKind::PartiallyMoved
                )
            }) || overlaps_active(*place, active, &function.places)
                || place_is_copy(*place, function, layouts);
            if unavailable {
                ownership_error(
                    instruction.span,
                    "drop targets a Copy, unavailable, or borrowed place",
                    errors,
                );
            } else {
                states[place.0 as usize] = PlaceState { kind: PlaceStateKind::Dropped };
                mark_ancestors_partial(*place, function, states);
            }
        }
        I::BeginBorrow(definition)
        | I::BeginIndexedBorrow { definition, .. }
        | I::BeginIndexedAccess { definition, .. } => {
            let index = definition.id.0 as usize;
            if state(definition.place, states)
                .is_none_or(|state| state.kind != PlaceStateKind::Initialized)
                || conflicts(definition, active, &function.places)
            {
                errors.push(error_at(
                    "ZRYNA-I3011",
                    instruction.span,
                    "borrow identity, owner state, or overlap is invalid",
                    "use a dense borrow ID and obey shared/exclusive overlap rules",
                ));
            } else {
                if active.len() <= index {
                    active.resize(index + 1, None);
                }
                if active[index].is_some() {
                    errors.push(error_at(
                        "ZRYNA-I3011",
                        instruction.span,
                        "borrow identity is defined more than once",
                        "define each borrow once",
                    ));
                } else {
                    active[index] = Some((definition.place, definition.access));
                }
            }
        }
        I::ProjectIndexedBorrow { parent, borrow, .. }
        | I::BindIndexedBorrow { parent, borrow } => {
            indexed_access::apply_projection(
                borrows,
                *parent,
                *borrow,
                instruction.span,
                active,
                errors,
            );
        }
        I::BorrowRead { borrow }
        | I::GenericCloneBorrow { borrow, .. }
        | I::HandleAwareCloneBorrow { borrow, .. } => {
            if active.get(borrow.0 as usize).is_none_or(Option::is_none) {
                errors.push(error_at(
                    "ZRYNA-I3011",
                    instruction.span,
                    "borrow read uses an inactive authority",
                    "read only through one active borrow",
                ));
            }
        }
        I::BorrowWrite { borrow, .. } | I::BorrowReplace { borrow, .. } => {
            if active
                .get(borrow.0 as usize)
                .and_then(|entry| *entry)
                .is_none_or(|(_, access)| access != raw::BorrowAccess::Exclusive)
            {
                errors.push(error_at(
                    "ZRYNA-I3011",
                    instruction.span,
                    "borrow write lacks an active exclusive authority",
                    "write only through one active exclusive borrow",
                ));
            }
            indexed_borrows::invalidate_borrow_variants(
                function,
                borrows,
                *borrow,
                active_variants,
            );
        }
        I::DirectCall { arguments, .. } => {
            if arguments.iter().any(|argument| {
                let raw::CallArgument::Borrow(borrow) = argument else {
                    return false;
                };
                active.get(borrow.0 as usize).is_none_or(Option::is_none)
            }) {
                errors.push(error_at(
                    "ZRYNA-I3011",
                    instruction.span,
                    "direct call uses an inactive borrow authority",
                    "pass only a borrow parameter or a currently active lexical borrow",
                ));
            }
        }
        I::EndBorrow { borrow }
            if (borrow.0 as usize) < function.borrow_parameters.len()
                || active.get_mut(borrow.0 as usize).and_then(Option::take).is_none() =>
        {
            errors.push(error_at(
                "ZRYNA-I3011",
                instruction.span,
                "borrow end uses an inactive authority",
                "end each active borrow exactly once",
            ));
        }
        _ => {}
    }
}

fn verify_cleanup(
    id: raw::CleanupPlanId,
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    flow: &OwnershipFlow,
    errors: &mut Errors,
) {
    let Some(plan) = function.cleanup_plans.get(id.0 as usize) else { return };
    let roots = flow.pending.iter().rev().copied().collect::<Vec<_>>();
    let expected = roots.iter().copied().map(raw::DropAction::DropPlace).collect::<Vec<_>>();
    let claimed = plan.actions.clone();
    if claimed != expected {
        errors.push(error_at(
            "ZRYNA-I3012",
            plan.span,
            "cleanup plan is incomplete, duplicated, or out of reverse-completion order",
            "drop every live non-Copy root exactly once in reverse completion order",
        ));
    }
    for root in &roots {
        for place in function.places.iter().filter(|place| {
            place.id == *root || is_projection_below(place.id, *root, &function.places)
        }) {
            let index = place.id.0 as usize;
            if matches!(
                flow.states[index].kind,
                PlaceStateKind::PartiallyInitialized | PlaceStateKind::PartiallyMoved
            ) && layout_type(layouts, function.places[index].ty)
                .is_some_and(|ty| ty.category() == TypeCategory::Enum)
                && flow.variants[index].is_none()
            {
                errors.push(error_at(
                    "ZRYNA-I3013",
                    plan.span,
                    "partial enum cleanup lacks an exact active-variant refinement",
                    "derive cleanup only while every partial enum's active variant is sealed",
                ));
            }
        }
    }
}

fn verify_vec_clone_element_cleanup(
    id: raw::CleanupPlanId,
    result_owner: Option<raw::PlaceId>,
    function: &raw::Function,
    flow: &OwnershipFlow,
    errors: &mut Errors,
) {
    let Some(plan) = function.cleanup_plans.get(id.0 as usize) else { return };
    let Some(result_owner) = result_owner else {
        errors.push(error_at(
            "ZRYNA-I3012",
            plan.span,
            "Vec clone element cleanup lacks its distinct destination owner",
            "bind the clone result to one exact temporary owner",
        ));
        return;
    };
    let expected = std::iter::once(raw::DropAction::DropVecInitializedPrefix(result_owner))
        .chain(flow.pending.iter().rev().copied().map(raw::DropAction::DropPlace))
        .collect::<Vec<_>>();
    if plan.actions != expected {
        errors.push(error_at(
            "ZRYNA-I3013",
            plan.span,
            "Vec clone element cleanup does not reverse-drop the completed destination prefix before live roots",
            "drop the initialized destination prefix first, then every pre-existing owner in reverse order",
        ));
    }
}

fn verify_aggregate_clone_element_cleanup(
    id: raw::CleanupPlanId,
    result_owner: Option<raw::PlaceId>,
    source: raw::PlaceId,
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    flow: &OwnershipFlow,
    errors: &mut Errors,
) {
    let Some(plan) = function.cleanup_plans.get(id.0 as usize) else { return };
    let Some(result_owner) = result_owner else {
        errors.push(error_at(
            "ZRYNA-I3012",
            plan.span,
            "aggregate clone element cleanup lacks its distinct destination owner",
            "bind the clone result to one exact temporary owner",
        ));
        return;
    };
    let expected = std::iter::once(raw::DropAction::DropAggregateInitializedPrefix(result_owner))
        .chain(flow.pending.iter().rev().copied().map(raw::DropAction::DropPlace))
        .collect::<Vec<_>>();
    if plan.actions != expected {
        errors.push(error_at(
            "ZRYNA-I3013",
            plan.span,
            "aggregate clone element cleanup does not reverse-drop the recursive destination prefix before live roots",
            "drop the initialized structural destination prefix first, then every pre-existing owner in reverse order",
        ));
    }
    let source_ty =
        function.places.get(source.0 as usize).and_then(|place| layout_type(layouts, place.ty));
    let active_variant = source_ty.and_then(|record| {
        (record.category() == TypeCategory::Enum)
            .then(|| flow.variants.get(source.0 as usize).copied().flatten())
            .flatten()
    });
    let has_exact_shape = source_ty.is_some_and(|record| {
        aggregate_clone_fallible_leaf_count(record.id(), layouts, active_variant, true).is_some()
    });
    if !has_exact_shape {
        errors.push(error_at(
            "ZRYNA-I3013",
            plan.span,
            "aggregate clone element cleanup lacks an authenticated active root shape",
            "clone a structurally supported source whose root Enum variant is known at this program point",
        ));
    }
}

fn ownership_error(span: Span, message: &'static str, errors: &mut Errors) {
    errors.push(error_at(
        "ZRYNA-I3010",
        span,
        message,
        "emit only legal ownership state transitions",
    ));
}
fn place_is_copy(place: raw::PlaceId, function: &raw::Function, layouts: &VerifiedLayouts) -> bool {
    function
        .places
        .get(place.0 as usize)
        .and_then(|place| layout_type(layouts, place.ty))
        .is_some_and(|ty| ty.drop_kind() == 0)
}
fn projection_base(kind: &raw::PlaceKind) -> Option<raw::PlaceId> {
    match kind {
        raw::PlaceKind::StructField { base, .. }
        | raw::PlaceKind::EnumPayload { base, .. }
        | raw::PlaceKind::FixedArrayConstant { base, .. } => Some(*base),
        _ => None,
    }
}

fn canonical_children(
    base: raw::PlaceId,
    function: &raw::Function,
    layouts: &VerifiedLayouts,
) -> Option<Vec<raw::PlaceId>> {
    let place = function.places.get(base.0 as usize)?;
    let ty = layout_type(layouts, place.ty)?;
    match ty.category() {
        TypeCategory::Struct => ty
            .fields()
            .iter()
            .map(|field| {
                function.places.iter().find_map(|place| match place.kind {
                    raw::PlaceKind::StructField { base: candidate, ordinal }
                        if candidate == base && ordinal == field.ordinal() =>
                    {
                        Some(place.id)
                    }
                    _ => None,
                })
            })
            .collect(),
        TypeCategory::FixedArray => {
            let length = usize::try_from(ty.array_length()?).ok()?;
            (0..length)
                .map(|index| {
                    let index = u32::try_from(index).ok()?;
                    function.places.iter().find_map(|place| match place.kind {
                        raw::PlaceKind::FixedArrayConstant {
                            base: candidate,
                            index: candidate_index,
                        } if candidate == base && candidate_index == index => Some(place.id),
                        _ => None,
                    })
                })
                .collect()
        }
        _ => None,
    }
}

fn enum_payload_ancestors(
    mut place: raw::PlaceId,
    function: &raw::Function,
) -> Vec<(raw::PlaceId, u32)> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    while seen.insert(place) {
        let Some(item) = function.places.get(place.0 as usize) else { break };
        if let raw::PlaceKind::EnumPayload { base, variant } = item.kind {
            out.push((base, variant));
        }
        let Some(base) = projection_base(&item.kind) else { break };
        place = base;
    }
    out
}

fn validate_projection_initialization(
    mut child: raw::PlaceId,
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    states: &[PlaceState],
    variants: &[Option<u32>],
    span: Span,
    errors: &mut Errors,
) -> bool {
    let mut valid = true;
    let mut seen = BTreeSet::new();
    while seen.insert(child) {
        let Some(item) = function.places.get(child.0 as usize) else { break };
        let Some(base) = projection_base(&item.kind) else { break };
        let Some(base_place) = function.places.get(base.0 as usize) else { break };
        let Some(base_ty) = layout_type(layouts, base_place.ty) else { break };
        match base_ty.category() {
            TypeCategory::Struct | TypeCategory::FixedArray => {
                let Some(children) = canonical_children(base, function, layouts) else {
                    errors.push(error_at(
                        "ZRYNA-I3013",
                        span,
                        "partial aggregate initialization lacks complete canonical child projections",
                        "declare every immediate field or fixed-array index before prefix initialization",
                    ));
                    return false;
                };
                let Some(selected) = children.iter().position(|candidate| *candidate == child)
                else {
                    errors.push(error_at(
                        "ZRYNA-I3013",
                        span,
                        "partial aggregate initialization does not follow a canonical child path",
                        "initialize one exact declared field or fixed-array index",
                    ));
                    return false;
                };
                let prefix = children[..selected]
                    .iter()
                    .all(|place| states[place.0 as usize].kind == PlaceStateKind::Initialized);
                let suffix = children[selected + 1..]
                    .iter()
                    .all(|place| states[place.0 as usize].kind == PlaceStateKind::Uninitialized);
                if !prefix || !suffix {
                    errors.push(error_at(
                        "ZRYNA-I3013",
                        span,
                        "aggregate projection initialization contains a hole or is out of order",
                        "commit fields or fixed-array elements in exact declaration or index order",
                    ));
                    valid = false;
                }
            }
            TypeCategory::Enum => {
                let raw::PlaceKind::EnumPayload { variant, .. } = item.kind else {
                    errors.push(error_at(
                        "ZRYNA-I3013",
                        span,
                        "partial enum initialization does not enter one payload projection",
                        "initialize only the exact payload projection of one variant",
                    ));
                    return false;
                };
                if variants[base.0 as usize].is_some_and(|active| active != variant) {
                    errors.push(error_at(
                        "ZRYNA-I3013",
                        span,
                        "enum payload initialization conflicts with the active variant",
                        "continue initializing only the already active variant payload",
                    ));
                    valid = false;
                }
            }
            _ => {}
        }
        child = base;
    }
    valid
}

fn promote_initialized_ancestors(
    mut child: raw::PlaceId,
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    states: &mut [PlaceState],
    variants: &mut [Option<u32>],
) {
    let mut seen = BTreeSet::new();
    while seen.insert(child) {
        let Some(item) = function.places.get(child.0 as usize) else { break };
        let Some(base) = projection_base(&item.kind) else { break };
        let Some(base_place) = function.places.get(base.0 as usize) else { break };
        let Some(base_ty) = layout_type(layouts, base_place.ty) else { break };
        states[base.0 as usize].kind = match base_ty.category() {
            TypeCategory::Struct | TypeCategory::FixedArray => {
                if canonical_children(base, function, layouts)
                    .filter(|children| !children.is_empty())
                    .is_some_and(|children| {
                        children.iter().all(|place| {
                            states[place.0 as usize].kind == PlaceStateKind::Initialized
                        })
                    })
                {
                    PlaceStateKind::Initialized
                } else {
                    PlaceStateKind::PartiallyInitialized
                }
            }
            TypeCategory::Enum => {
                if let raw::PlaceKind::EnumPayload { variant, .. } = item.kind {
                    variants[base.0 as usize] = Some(variant);
                }
                if states[child.0 as usize].kind == PlaceStateKind::Initialized {
                    PlaceStateKind::Initialized
                } else {
                    PlaceStateKind::PartiallyInitialized
                }
            }
            _ => states[base.0 as usize].kind,
        };
        child = base;
    }
}
fn initialize_projections(root: raw::PlaceId, function: &raw::Function, states: &mut [PlaceState]) {
    let mut visited = vec![false; function.places.len()];
    let mut queue = VecDeque::from([root]);
    while let Some(parent) = queue.pop_front() {
        let Ok(parent_index) = usize::try_from(parent.0) else { continue };
        if parent_index >= visited.len() || visited[parent_index] {
            continue;
        }
        visited[parent_index] = true;
        for (index, place) in function.places.iter().enumerate() {
            if projection_base(&place.kind) == Some(parent) && !visited[index] {
                states[index] = PlaceState { kind: PlaceStateKind::Initialized };
                queue.push_back(raw::PlaceId(u32::try_from(index).expect("bounded place index")));
            }
        }
    }
}

fn mark_ancestors_partial(
    mut place: raw::PlaceId,
    function: &raw::Function,
    states: &mut [PlaceState],
) {
    let mut visited = vec![false; function.places.len()];
    while let Some(base) =
        function.places.get(place.0 as usize).and_then(|place| projection_base(&place.kind))
    {
        let Ok(index) = usize::try_from(base.0) else { break };
        if index >= visited.len() || visited[index] {
            break;
        }
        visited[index] = true;
        if states[index].kind == PlaceStateKind::Initialized {
            states[index].kind = PlaceStateKind::PartiallyMoved;
        }
        place = base;
    }
}
fn places_overlap(left: raw::PlaceId, right: raw::PlaceId, places: &[raw::Place]) -> bool {
    fn is_ancestor_or_same(
        ancestor: raw::PlaceId,
        mut place: raw::PlaceId,
        places: &[raw::Place],
    ) -> bool {
        let mut visited = vec![false; places.len()];
        for _ in 0..=MAX_PLACES_PER_FUNCTION {
            if place == ancestor {
                return true;
            }
            let Ok(index) = usize::try_from(place.0) else { return false };
            if index >= visited.len() || visited[index] {
                return false;
            }
            visited[index] = true;
            let Some(base) = places.get(index).and_then(|item| projection_base(&item.kind)) else {
                return false;
            };
            place = base;
        }
        false
    }
    is_ancestor_or_same(left, right, places) || is_ancestor_or_same(right, left, places)
}
fn overlaps_active(
    place: raw::PlaceId,
    active: &[Option<(raw::PlaceId, raw::BorrowAccess)>],
    places: &[raw::Place],
) -> bool {
    active.iter().flatten().any(|(borrowed, _)| places_overlap(place, *borrowed, places))
}
fn overlaps_exclusive(
    place: raw::PlaceId,
    active: &[Option<(raw::PlaceId, raw::BorrowAccess)>],
    places: &[raw::Place],
) -> bool {
    active.iter().flatten().any(|(borrowed, access)| {
        *access == raw::BorrowAccess::Exclusive && places_overlap(place, *borrowed, places)
    })
}
fn conflicts(
    definition: &raw::BorrowDefinition,
    active: &[Option<(raw::PlaceId, raw::BorrowAccess)>],
    places: &[raw::Place],
) -> bool {
    active.iter().flatten().any(|(place, access)| {
        places_overlap(definition.place, *place, places)
            && (definition.access == raw::BorrowAccess::Exclusive
                || *access == raw::BorrowAccess::Exclusive)
    })
}

fn compute_dominators(predecessors: &[Vec<usize>], reachable: &[bool]) -> Vec<BTreeSet<usize>> {
    let all = (0..predecessors.len()).filter(|index| reachable[*index]).collect::<BTreeSet<_>>();
    let mut result = vec![all.clone(); predecessors.len()];
    result[0] = BTreeSet::from([0]);
    loop {
        let mut changed = false;
        for block in 1..predecessors.len() {
            if !reachable[block] {
                continue;
            }
            let mut next = all.clone();
            for predecessor in &predecessors[block] {
                next = next.intersection(&result[*predecessor]).copied().collect();
            }
            next.insert(block);
            if next != result[block] {
                result[block] = next;
                changed = true;
            }
        }
        if !changed {
            return result;
        }
    }
}

fn verify_use(
    value: raw::ValueId,
    block: usize,
    position: usize,
    span: Span,
    values: &[ValueInfo],
    dominators: &[BTreeSet<usize>],
    errors: &mut Errors,
) {
    let Some(info) = value_info(values, value) else {
        errors.push(error_at(
            "ZRYNA-I3008",
            span,
            "operation references a foreign or undefined value",
            "use a value defined by the containing function",
        ));
        return;
    };
    let dominates = match info.site {
        DefinitionSite::FunctionParameter => true,
        DefinitionSite::BlockParameter(definition_block) => {
            dominators[block].contains(&definition_block)
        }
        DefinitionSite::Instruction(definition_block, definition_position) => {
            definition_block == block && definition_position < position
                || definition_block != block && dominators[block].contains(&definition_block)
        }
    };
    if !dominates {
        errors.push(error_at(
            "ZRYNA-I3008",
            span,
            "value definition does not dominate its use",
            "use only earlier same-block or dominating definitions",
        ));
    }
}

fn aggregate_clone_fallible_leaf_count(
    ty: zryna_layout::TypeId,
    layouts: &VerifiedLayouts,
    root_variant: Option<u32>,
    root: bool,
) -> Option<u64> {
    let record = layouts.type_by_id(ty)?;
    match record.category() {
        TypeCategory::Bool | TypeCategory::I32 => Some(0),
        TypeCategory::String => Some(1),
        TypeCategory::Struct => record.fields().iter().try_fold(0_u64, |total, field| {
            total.checked_add(aggregate_clone_fallible_leaf_count(
                field.ty(),
                layouts,
                None,
                false,
            )?)
        }),
        TypeCategory::FixedArray => record.array_length()?.checked_mul(
            aggregate_clone_fallible_leaf_count(record.referenced_type()?, layouts, None, false)?,
        ),
        TypeCategory::Enum if root => {
            let variant = usize::try_from(root_variant?).ok()?;
            record.variants().get(variant)?.payload().map_or(Some(0), |payload| {
                aggregate_clone_fallible_leaf_count(payload, layouts, None, false)
            })
        }
        TypeCategory::Enum | TypeCategory::Vec | TypeCategory::Shared | TypeCategory::Weak => None,
    }
}

fn structural_clone_capable(ty: raw::TypeId, layouts: &VerifiedLayouts) -> bool {
    fn visit(
        ty: zryna_layout::TypeId,
        layouts: &VerifiedLayouts,
        active: &mut BTreeSet<u32>,
        root: bool,
    ) -> bool {
        if !active.insert(ty.index()) {
            return false;
        }
        let Some(record) = layouts.type_by_id(ty) else {
            return false;
        };
        let capable = match record.category() {
            TypeCategory::Bool | TypeCategory::I32 | TypeCategory::String => true,
            TypeCategory::Struct => {
                record.fields().iter().all(|field| visit(field.ty(), layouts, active, false))
            }
            TypeCategory::Enum if root => record.variants().iter().all(|variant| {
                variant.payload().is_none_or(|payload| visit(payload, layouts, active, false))
            }),
            TypeCategory::FixedArray => record
                .referenced_type()
                .is_some_and(|element| visit(element, layouts, active, false)),
            TypeCategory::Enum | TypeCategory::Vec | TypeCategory::Shared | TypeCategory::Weak => {
                false
            }
        };
        active.remove(&ty.index());
        capable
    }

    let Some(ty) = layout_type(layouts, ty).map(zryna_layout::VerifiedType::id) else {
        return false;
    };
    matches!(
        layouts.type_by_id(ty).map(zryna_layout::VerifiedType::category),
        Some(TypeCategory::Struct | TypeCategory::Enum | TypeCategory::FixedArray)
    ) && visit(ty, layouts, &mut BTreeSet::new(), true)
}

#[allow(clippy::too_many_lines)]
fn verify_operation_types(
    instruction: &raw::Instruction,
    function: &raw::Function,
    values: &[ValueInfo],
    layouts: &VerifiedLayouts,
    clone_types: (&[bool], &[bool]),
    borrows: &BorrowIndex,
    errors: &mut Errors,
) {
    use raw::InstructionKind as I;

    let category = |value: raw::ValueId| {
        value_info(values, value)
            .and_then(|info| layout_type(layouts, info.ty))
            .map(zryna_layout::VerifiedType::category)
    };
    let result_type = instruction.result.map(|value| value.ty);
    let result = result_type
        .and_then(|ty| layout_type(layouts, ty))
        .map(zryna_layout::VerifiedType::category);
    let place_type =
        |place: raw::PlaceId| function.places.get(place.0 as usize).map(|place| place.ty);
    let valid = match &instruction.kind {
        I::BoolLiteral(_) => result == Some(TypeCategory::Bool),
        I::I32Literal(_) => result == Some(TypeCategory::I32),
        I::I32Add { lhs, rhs } | I::I32Sub { lhs, rhs } | I::I32Mul { lhs, rhs } => {
            category(*lhs) == Some(TypeCategory::I32)
                && category(*rhs) == Some(TypeCategory::I32)
                && result == Some(TypeCategory::I32)
        }
        I::I32Neg { operand } => {
            category(*operand) == Some(TypeCategory::I32) && result == Some(TypeCategory::I32)
        }
        I::I32LtS { lhs, rhs }
        | I::I32LeS { lhs, rhs }
        | I::I32GtS { lhs, rhs }
        | I::I32GeS { lhs, rhs } => {
            category(*lhs) == Some(TypeCategory::I32)
                && category(*rhs) == Some(TypeCategory::I32)
                && result == Some(TypeCategory::Bool)
        }
        I::Eq { lhs, rhs } | I::Ne { lhs, rhs } => {
            value_info(values, *lhs).zip(value_info(values, *rhs)).is_some_and(|(left, right)| {
                left.ty == right.ty
                    && matches!(category(*lhs), Some(TypeCategory::Bool | TypeCategory::I32))
                    && result == Some(TypeCategory::Bool)
            })
        }
        I::StructConstruct { fields, cleanup } => {
            cleanup.is_none()
                && result_type.and_then(|ty| layout_type(layouts, ty)).is_some_and(|ty| {
                    ty.category() == TypeCategory::Struct
                        && fields.len() == ty.fields().len()
                        && fields.iter().zip(ty.fields()).all(|(value, field)| {
                            value_info(values, *value)
                                .is_some_and(|info| info.ty.0 == field.ty().index())
                        })
                })
        }
        I::EnumConstruct { variant, payload, cleanup } => {
            cleanup.is_none()
                && result_type
                    .and_then(|ty| layout_type(layouts, ty))
                    .and_then(|ty| ty.variants().get(*variant as usize).copied())
                    .is_some_and(|record| match (payload, record.payload()) {
                        (None, None) => true,
                        (Some(value), Some(ty)) => {
                            value_info(values, *value).is_some_and(|info| info.ty.0 == ty.index())
                        }
                        _ => false,
                    })
        }
        I::FixedArrayConstruct { elements, cleanup } => {
            cleanup.is_none()
                && result_type.and_then(|ty| layout_type(layouts, ty)).is_some_and(|ty| {
                    ty.category() == TypeCategory::FixedArray
                        && ty.array_length() == Some(elements.len() as u64)
                        && ty.referenced_type().is_some_and(|element| {
                            elements.iter().all(|value| {
                                value_info(values, *value)
                                    .is_some_and(|info| info.ty.0 == element.index())
                            })
                        })
                })
        }
        I::EnvironmentLookup { .. } => {
            result_type.is_some_and(|ty| crate::command_h1_v1::outcome_shape(layouts, ty).is_some())
        }
        I::CopyFromPlace { place } | I::MoveFromPlace { place } => {
            place_type(*place) == result_type
        }
        I::ClonePlace { place, element_cleanup, .. } => {
            place_type(*place) == result_type
                && result_type.is_some_and(|ty| {
                    structural_clone_capable(ty, layouts)
                        && layout_type(layouts, ty).is_some_and(|record| {
                            (record.drop_kind() == 0) == element_cleanup.is_none()
                        })
                })
        }
        I::GenericClonePlace { place, .. } => {
            place_type(*place) == result_type
                && result_type
                    .is_some_and(|ty| clone_types.0.get(ty.0 as usize).copied().unwrap_or(false))
        }
        I::GenericCloneBorrow { borrow, .. } => {
            borrows.definition(*borrow).is_some_and(|(referent, _)| {
                Some(referent) == result_type
                    && clone_types.0.get(referent.0 as usize).copied().unwrap_or(false)
            })
        }
        I::HandleAwareClonePlace { place, .. } => {
            place_type(*place) == result_type
                && result_type
                    .is_some_and(|ty| clone_types.1.get(ty.0 as usize).copied().unwrap_or(false))
        }
        I::HandleAwareCloneBorrow { borrow, .. } => {
            borrows.definition(*borrow).is_some_and(|(referent, _)| {
                Some(referent) == result_type
                    && clone_types.1.get(referent.0 as usize).copied().unwrap_or(false)
            })
        }
        I::GenericMoveFromPlace { place } => {
            place_type(*place) == result_type
                && generic_static_places::valid_move_type(*place, function, clone_types)
        }
        I::GenericReplacePlace { place, value } => {
            instruction.result.is_none()
                && place_type(*place) == value_info(values, *value).map(|info| info.ty)
                && generic_static_places::valid_type(*place, function, clone_types)
        }
        I::InitializePlace { place, value } | I::ReplacePlace { place, value, .. } => {
            place_type(*place) == value_info(values, *value).map(|info| info.ty)
        }
        I::BorrowWrite { borrow, value } => {
            instruction.result.is_none()
                && borrows.definition(*borrow).is_some_and(|(referent, access)| {
                    access == raw::BorrowAccess::Exclusive
                        && layout_type(layouts, referent).is_some_and(|ty| ty.drop_kind() == 0)
                        && value_info(values, *value).is_some_and(|info| info.ty == referent)
                })
        }
        I::BeginIndexedBorrow { definition, index, .. }
        | I::BeginIndexedAccess { definition, index, .. } => {
            instruction.result.is_none()
                && indexed_borrows::element_type(function, definition.place, layouts).is_some()
                && value_info(values, *index)
                    .and_then(|info| layout_type(layouts, info.ty))
                    .is_some_and(|ty| ty.category() == TypeCategory::I32)
        }
        I::ProjectIndexedBorrow { parent, borrow, index, .. } => {
            instruction.result.is_none()
                && parent.0 < borrow.0
                && borrows.origin(*parent).is_some()
                && borrows.definition(*parent).is_some_and(|(ty, _)| {
                    indexed_borrows::container_element_type(ty, layouts).is_some()
                })
                && value_info(values, *index)
                    .and_then(|info| layout_type(layouts, info.ty))
                    .is_some_and(|ty| ty.category() == TypeCategory::I32)
        }
        I::BindIndexedBorrow { parent, borrow } => {
            instruction.result.is_none()
                && parent.0 < borrow.0
                && borrows.origin(*parent).is_some()
                && borrows.definition(*parent).is_some()
        }
        I::BorrowReplace { borrow, value } => {
            instruction.result.is_none()
                && borrows.definition(*borrow).is_some_and(|(referent, access)| {
                    access == raw::BorrowAccess::Exclusive
                        && layout_type(layouts, referent).is_some_and(|ty| ty.drop_kind() != 0)
                        && value_info(values, *value).is_some_and(|info| info.ty == referent)
                })
        }
        I::DropPlace { .. } | I::BeginBorrow(_) | I::EndBorrow { .. } => {
            instruction.result.is_none()
        }
        I::EnumDiscriminant { place } => {
            place_type(*place)
                .and_then(|ty| layout_type(layouts, ty))
                .is_some_and(|ty| ty.category() == TypeCategory::Enum)
                && result == Some(TypeCategory::I32)
        }
        I::FixedArrayIndexCopy { place, index, .. } => indexed_result(
            *place,
            *index,
            TypeCategory::FixedArray,
            function,
            values,
            layouts,
            result_type,
        ),
        I::VecIndexCopy { place, index, .. } => indexed_result(
            *place,
            *index,
            TypeCategory::Vec,
            function,
            values,
            layouts,
            result_type,
        ),
        I::StringFromUtf8 { bytes, .. } => {
            result == Some(TypeCategory::String) && std::str::from_utf8(bytes).is_ok()
        }
        I::StringClone { place, .. } => {
            place_type(*place) == result_type && result == Some(TypeCategory::String)
        }
        I::StringConcat { left, right, .. } => {
            place_type(*left) == result_type
                && place_type(*right) == result_type
                && result == Some(TypeCategory::String)
        }
        I::VecClone { place, element_cleanup, .. } => {
            place_type(*place) == result_type
                && result_type.and_then(|ty| layout_type(layouts, ty)).is_some_and(|ty| {
                    ty.category() == TypeCategory::Vec
                        && ty.referenced_type().is_some_and(|element| {
                            layouts.type_by_id(element).is_some_and(|element| {
                                match element.category() {
                                    TypeCategory::Bool | TypeCategory::I32 => {
                                        element.drop_kind() == 0 && element_cleanup.is_none()
                                    }
                                    TypeCategory::String => {
                                        element.drop_kind() != 0 && element_cleanup.is_some()
                                    }
                                    _ => false,
                                }
                            })
                        })
                })
        }
        I::VecConstruct { elements, .. } => {
            container_elements(elements, TypeCategory::Vec, result_type, values, layouts)
        }
        I::VecPush { vector, value, .. } => {
            instruction.result.is_none()
                && function
                    .places
                    .get(vector.0 as usize)
                    .and_then(|place| layout_type(layouts, place.ty))
                    .is_some_and(|ty| {
                        ty.category() == TypeCategory::Vec
                            && ty.referenced_type().is_some_and(|element| {
                                value_info(values, *value)
                                    .is_some_and(|info| info.ty.0 == element.index())
                            })
                    })
        }
        I::SharedConstruct { value, .. } => {
            result_type.and_then(|ty| layout_type(layouts, ty)).is_some_and(|ty| {
                ty.category() == TypeCategory::Shared
                    && ty.referenced_type().is_some_and(|payload| {
                        value_info(values, *value).is_some_and(|info| info.ty.0 == payload.index())
                    })
            })
        }
        I::SharedClone { place, .. } => {
            place_type(*place) == result_type && result == Some(TypeCategory::Shared)
        }
        I::WeakDowngrade { place, .. } => place_type(*place)
            .and_then(|ty| layout_type(layouts, ty))
            .zip(result_type.and_then(|ty| layout_type(layouts, ty)))
            .is_some_and(|(shared, weak)| {
                shared.category() == TypeCategory::Shared
                    && weak.category() == TypeCategory::Weak
                    && shared.referenced_type() == weak.referenced_type()
            }),
        I::WeakClone { place, .. } => {
            place_type(*place) == result_type && result == Some(TypeCategory::Weak)
        }
        I::BorrowRead { borrow } => borrows.definition(*borrow).is_some_and(|(ty, _)| {
            result_type == Some(ty)
                && layout_type(layouts, ty).is_some_and(|ty| ty.drop_kind() == 0)
        }),
        I::DirectCall { .. } => true,
    };
    if !valid {
        errors.push(error_at(
            "ZRYNA-I3005",
            instruction.span,
            "instruction operands or result have an invalid sealed type",
            "use the exact operand and result types required by the operation",
        ));
    }
}

fn value_info(values: &[ValueInfo], id: raw::ValueId) -> Option<ValueInfo> {
    usize::try_from(id.0).ok().and_then(|index| values.get(index)).copied()
}

fn indexed_result(
    place: raw::PlaceId,
    index: raw::ValueId,
    category: TypeCategory,
    function: &raw::Function,
    values: &[ValueInfo],
    layouts: &VerifiedLayouts,
    result: Option<raw::TypeId>,
) -> bool {
    value_info(values, index)
        .and_then(|info| layout_type(layouts, info.ty))
        .is_some_and(|ty| ty.category() == TypeCategory::I32)
        && function
            .places
            .get(place.0 as usize)
            .and_then(|place| layout_type(layouts, place.ty))
            .is_some_and(|ty| {
                ty.category() == category
                    && ty.referenced_type().is_some_and(|element| {
                        result.is_some_and(|result| result.0 == element.index())
                            && layout_type(layouts, raw::TypeId(element.index()))
                                .is_some_and(|element| element.drop_kind() == 0)
                    })
            })
}

fn container_elements(
    elements: &[raw::ValueId],
    category: TypeCategory,
    result: Option<raw::TypeId>,
    values: &[ValueInfo],
    layouts: &VerifiedLayouts,
) -> bool {
    result.and_then(|ty| layout_type(layouts, ty)).is_some_and(|ty| {
        ty.category() == category
            && ty.referenced_type().is_some_and(|element| {
                elements.iter().all(|value| {
                    value_info(values, *value).is_some_and(|info| info.ty.0 == element.index())
                })
            })
    })
}

fn terminator_edges(kind: &raw::Terminator) -> Vec<&raw::Edge> {
    match kind {
        raw::Terminator::Return { .. } | raw::Terminator::Trap { .. } => vec![],
        raw::Terminator::Jump(edge) => vec![edge],
        raw::Terminator::Branch { when_true, when_false, .. } => vec![when_true, when_false],
        raw::Terminator::EnumMatch { arms, .. } => arms.iter().map(|arm| &arm.edge).collect(),
        raw::Terminator::WeakUpgradeBranch { success, expired, .. } => vec![success, expired],
    }
}

fn terminator_operands(kind: &raw::Terminator) -> Vec<raw::ValueId> {
    let mut values = terminator_edges(kind)
        .into_iter()
        .flat_map(|edge| edge.arguments.iter().copied())
        .collect::<Vec<_>>();
    match kind {
        raw::Terminator::Return { value, .. } => values.push(*value),
        raw::Terminator::Branch { condition, .. } => values.push(*condition),
        _ => {}
    }
    values
}

fn verify_instruction_shape(
    instruction: &raw::Instruction,
    function: &raw::Function,
    errors: &mut Errors,
) {
    use raw::InstructionKind as I;
    let effect_only = matches!(
        instruction.kind,
        I::InitializePlace { .. }
            | I::ReplacePlace { .. }
            | I::GenericReplacePlace { .. }
            | I::DropPlace { .. }
            | I::VecPush { .. }
            | I::BeginBorrow(_)
            | I::BeginIndexedBorrow { .. }
            | I::BeginIndexedAccess { .. }
            | I::ProjectIndexedBorrow { .. }
            | I::BindIndexedBorrow { .. }
            | I::BorrowReplace { .. }
            | I::BorrowWrite { .. }
            | I::EndBorrow { .. }
    );
    if effect_only == instruction.result.is_some() {
        errors.push(error_at(
            "ZRYNA-I3005",
            instruction.span,
            "instruction result shape does not match its operation",
            "emit a result exactly for value-producing operations",
        ));
    }
    let place_valid = |place: raw::PlaceId| (place.0 as usize) < function.places.len();
    let cleanup_valid = |plan: raw::CleanupPlanId| (plan.0 as usize) < function.cleanup_plans.len();
    let bad = match &instruction.kind {
        I::CopyFromPlace { place }
        | I::MoveFromPlace { place }
        | I::GenericMoveFromPlace { place }
        | I::DropPlace { place }
        | I::EnumDiscriminant { place }
        | I::ReplacePlace { place, .. }
        | I::GenericReplacePlace { place, .. } => !place_valid(*place),
        I::ClonePlace { place, cleanup, element_cleanup } => {
            !place_valid(*place)
                || !cleanup_valid(*cleanup)
                || element_cleanup.is_some_and(|cleanup| !cleanup_valid(cleanup))
        }
        I::GenericClonePlace { place, cleanup, prefix_cleanup }
        | I::HandleAwareClonePlace { place, cleanup, prefix_cleanup } => {
            !place_valid(*place) || !cleanup_valid(*cleanup) || !cleanup_valid(*prefix_cleanup)
        }
        I::GenericCloneBorrow { cleanup, prefix_cleanup, .. }
        | I::HandleAwareCloneBorrow { cleanup, prefix_cleanup, .. } => {
            !cleanup_valid(*cleanup) || !cleanup_valid(*prefix_cleanup)
        }
        I::FixedArrayIndexCopy { place, cleanup, .. }
        | I::VecIndexCopy { place, cleanup, .. }
        | I::StringClone { place, cleanup }
        | I::SharedClone { place, cleanup }
        | I::WeakDowngrade { place, cleanup }
        | I::WeakClone { place, cleanup } => !place_valid(*place) || !cleanup_valid(*cleanup),
        I::VecClone { place, cleanup, element_cleanup } => {
            !place_valid(*place)
                || !cleanup_valid(*cleanup)
                || element_cleanup.is_some_and(|cleanup| !cleanup_valid(cleanup))
        }
        I::StringFromUtf8 { cleanup, .. }
        | I::DirectCall { cleanup, .. }
        | I::VecConstruct { cleanup, .. }
        | I::EnvironmentLookup { cleanup, .. }
        | I::ProjectIndexedBorrow { cleanup, .. }
        | I::SharedConstruct { cleanup, .. } => !cleanup_valid(*cleanup),
        I::StringConcat { left, right, cleanup } => {
            !place_valid(*left) || !place_valid(*right) || !cleanup_valid(*cleanup)
        }
        I::VecPush { vector, cleanup, .. } => !place_valid(*vector) || !cleanup_valid(*cleanup),
        I::BeginBorrow(def) => !place_valid(def.place),
        I::BeginIndexedBorrow { definition, cleanup, .. }
        | I::BeginIndexedAccess { definition, cleanup, .. } => {
            !place_valid(definition.place) || !cleanup_valid(*cleanup)
        }
        _ => false,
    };
    if bad {
        errors.push(error_at(
            "ZRYNA-I3006",
            instruction.span,
            "instruction references a foreign place or cleanup plan",
            "use identities owned by the containing function",
        ));
    }
}

fn verify_terminator(
    terminator: &raw::SpannedTerminator,
    function: &raw::Function,
    errors: &mut Errors,
) {
    let block_ok = |edge: &raw::Edge| {
        (edge.target.0 as usize) < function.blocks.len()
            && edge.arguments.len() <= MAX_BLOCK_PARAMETERS
    };
    let cleanup_ok = |id: raw::CleanupPlanId| (id.0 as usize) < function.cleanup_plans.len();
    let valid = match &terminator.kind {
        raw::Terminator::Return { cleanup, .. } | raw::Terminator::Trap { cleanup, .. } => {
            cleanup_ok(*cleanup)
        }
        raw::Terminator::Jump(edge) => block_ok(edge),
        raw::Terminator::Branch { when_true, when_false, .. } => {
            block_ok(when_true) && block_ok(when_false)
        }
        raw::Terminator::EnumMatch { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|arm| block_ok(&arm.edge))
        }
        raw::Terminator::WeakUpgradeBranch { success, expired, cleanup, .. } => {
            block_ok(success) && block_ok(expired) && cleanup_ok(*cleanup)
        }
    };
    if !valid {
        errors.push(error_at(
            "ZRYNA-I3007",
            terminator.span,
            "terminator contains an invalid edge or cleanup identity",
            "use local blocks, bounded edge arguments, and local cleanup plans",
        ));
    }
}

fn check_value(
    value: &raw::ValueDefinition,
    expected: &mut u32,
    file: FileId,
    sources: &SourceMap,
    layouts: &VerifiedLayouts,
    errors: &mut Errors,
) {
    if value.id.0 != *expected {
        errors.push(error_at(
            "ZRYNA-I3002",
            value.span,
            "value definitions are not in canonical dense order",
            "allocate parameters, block parameters, and results in canonical order",
        ));
    }
    *expected = expected.saturating_add(1);
    if layout_type(layouts, value.ty).is_none() {
        errors.push(error_at(
            "ZRYNA-I3003",
            value.span,
            "value names an unknown layout TypeId",
            "use a type from the exact sealed universe",
        ));
    }
    check_span(value.span, file, sources, errors);
}

fn verify_projection(
    place: &raw::Place,
    function: &raw::Function,
    layouts: &VerifiedLayouts,
    errors: &mut Errors,
) {
    let places = &function.places;
    let valid = match place.kind {
        raw::PlaceKind::Parameter(parameter) => {
            function.parameters.get(parameter as usize).is_some_and(|value| value.ty == place.ty)
        }
        raw::PlaceKind::Local(local) => {
            local < u32::try_from(MAX_PLACES_PER_FUNCTION).expect("place limit fits u32")
        }
        raw::PlaceKind::Temporary(value) => function_value_type(function, value) == Some(place.ty),
        raw::PlaceKind::StructField { base, ordinal } => (base.0 < place.id.0)
            .then(|| places.get(base.0 as usize))
            .flatten()
            .and_then(|base| layout_type(layouts, base.ty))
            .and_then(|ty| ty.fields().get(ordinal as usize).copied())
            .is_some_and(|field| field.ordinal() == ordinal && field.ty().index() == place.ty.0),
        raw::PlaceKind::EnumPayload { base, variant } => (base.0 < place.id.0)
            .then(|| places.get(base.0 as usize))
            .flatten()
            .and_then(|base| layout_type(layouts, base.ty))
            .and_then(|ty| ty.variants().get(variant as usize).copied())
            .and_then(zryna_layout::VerifiedVariant::payload)
            .is_some_and(|ty| ty.index() == place.ty.0),
        raw::PlaceKind::FixedArrayConstant { base, index } => (base.0 < place.id.0)
            .then(|| places.get(base.0 as usize))
            .flatten()
            .and_then(|base| layout_type(layouts, base.ty))
            .is_some_and(|ty| {
                ty.array_length().is_some_and(|length| u64::from(index) < length)
                    && ty.referenced_type().is_some_and(|element| element.index() == place.ty.0)
            }),
    };
    if !valid {
        errors.push(error_at(
            "ZRYNA-I3006",
            place.span,
            "place projection does not match its sealed layout",
            "use an in-range source ordinal and the exact projected type",
        ));
    }
}

#[allow(clippy::too_many_lines)]
fn verify_calls(program: &raw::Program, borrow_indices: &[Vec<BorrowIndex>], errors: &mut Errors) {
    let function_count = program.modules.iter().map(|module| module.functions.len()).sum();
    let mut offsets = Vec::with_capacity(program.modules.len());
    let mut next = 0usize;
    for module in &program.modules {
        offsets.push(next);
        next += module.functions.len();
    }
    let mut graph = vec![Vec::<usize>::new(); function_count];
    for (module_index, module) in program.modules.iter().enumerate() {
        for (function_index, function) in module.functions.iter().enumerate() {
            let caller = offsets[module_index] + function_index;
            let borrows = &borrow_indices[module_index][function_index];
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                let raw::InstructionKind::DirectCall { callee, arguments, .. } = &instruction.kind
                else {
                    continue;
                };
                let Some(callee_module) = program.modules.get(callee.module.0 as usize) else {
                    call_error_at(instruction.span, "direct call names an unknown module", errors);
                    continue;
                };
                let Some(target) = callee_module.functions.get(callee.declaration as usize) else {
                    call_error_at(
                        instruction.span,
                        "direct call names an unknown function",
                        errors,
                    );
                    continue;
                };
                if target.id != *callee
                    || arguments.len() != target.parameters.len() + target.borrow_parameters.len()
                {
                    call_error_at(
                        instruction.span,
                        "direct call identity or arity does not match its callee",
                        errors,
                    );
                    continue;
                }
                let mut valid = true;
                let mut exclusive_arguments = Vec::<(raw::BorrowId, Option<raw::PlaceId>)>::new();
                for (argument, parameter) in arguments.iter().zip(&target.parameters) {
                    let raw::CallArgument::Value(value) = argument else {
                        valid = false;
                        break;
                    };
                    valid &= function_value_type(function, *value) == Some(parameter.ty);
                }
                for (argument, parameter) in
                    arguments.iter().skip(target.parameters.len()).zip(&target.borrow_parameters)
                {
                    let raw::CallArgument::Borrow(borrow) = argument else {
                        valid = false;
                        break;
                    };
                    valid &= borrows.definition(*borrow).is_some_and(|(referent, access)| {
                        referent == parameter.referent && access == parameter.access
                    });
                    if parameter.access == raw::BorrowAccess::Exclusive {
                        let place = borrows.region(*borrow);
                        if exclusive_arguments.iter().any(|(prior, prior_place)| {
                            prior == borrow
                                || prior_place.zip(place).is_some_and(|(left, right)| {
                                    places_overlap(left, right, &function.places)
                                })
                        }) {
                            valid = false;
                        }
                        exclusive_arguments.push((*borrow, place));
                    }
                }
                valid &= instruction.result.is_some_and(|result| result.ty == target.result);
                if !valid {
                    call_error_at(
                        instruction.span,
                        "direct call argument or result type does not match its callee",
                        errors,
                    );
                }
                graph[caller].push(offsets[callee.module.0 as usize] + callee.declaration as usize);
            }
        }
    }
    let mut indegree = vec![0usize; function_count];
    for edges in &graph {
        for target in edges {
            indegree[*target] += 1;
        }
    }
    let mut queue = VecDeque::new();
    let mut depth = vec![1usize; function_count];
    for (index, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            queue.push_back(index);
        }
    }
    let mut visited = 0usize;
    while let Some(function) = queue.pop_front() {
        visited += 1;
        for target in &graph[function] {
            depth[*target] = depth[*target].max(depth[function].saturating_add(1));
            if depth[*target] > MAX_STATIC_CALL_DEPTH {
                call_error("static call depth exceeds its limit", errors);
                return;
            }
            indegree[*target] -= 1;
            if indegree[*target] == 0 {
                queue.push_back(*target);
            }
        }
    }
    if visited != function_count {
        call_error("direct call graph contains a cycle", errors);
    }
}

fn function_value_type(function: &raw::Function, id: raw::ValueId) -> Option<raw::TypeId> {
    let mut values = function.parameters.iter().map(|value| value.ty).collect::<Vec<_>>();
    for block in &function.blocks {
        values.extend(block.parameters.iter().map(|value| value.ty));
        values.extend(
            block
                .instructions
                .iter()
                .filter_map(|instruction| instruction.result.map(|value| value.ty)),
        );
    }
    values.get(id.0 as usize).copied()
}

fn call_error(message: &'static str, errors: &mut Errors) {
    errors.push(error("ZRYNA-I3009", message, "use one acyclic exact-signature direct call graph"));
}
fn call_error_at(span: Span, message: &'static str, errors: &mut Errors) {
    errors.push(error_at(
        "ZRYNA-I3009",
        span,
        message,
        "use one acyclic exact-signature direct call graph",
    ));
}

fn verify_public_abi(
    program: &raw::Program,
    layouts: &VerifiedLayouts,
    errors: &mut Errors,
) -> (Option<zryna_abi::VerifiedScalarAbiModule>, Vec<Vec<Option<usize>>>) {
    let mut exports = Vec::new();
    let mut indices = Vec::new();
    let mut first_export_span = None;
    for module in &program.modules {
        let mut module_indices = Vec::new();
        for function in &module.functions {
            if let Some(name) = &function.entry_export {
                first_export_span.get_or_insert(function.span);
                if module.id != program.entry_module || !function.borrow_parameters.is_empty() {
                    errors.push(error_at(
                        "ZRYNA-I3009",
                        function.span,
                        "only borrow-free entry-module functions may claim public exports",
                        "keep aggregate and borrowed functions internal",
                    ));
                    module_indices.push(None);
                    continue;
                }
                let Some(parameters) = function
                    .parameters
                    .iter()
                    .map(|p| abi_type(layouts, p.ty))
                    .collect::<Option<Vec<_>>>()
                else {
                    errors.push(error_at(
                        "ZRYNA-I3009",
                        function.span,
                        "public export contains a non-scalar parameter",
                        "public DataOwnershipV1 ABI remains scalar bool/i32 v1",
                    ));
                    module_indices.push(None);
                    continue;
                };
                let Some(result) = abi_type(layouts, function.result) else {
                    errors.push(error_at(
                        "ZRYNA-I3009",
                        function.span,
                        "public export contains a non-scalar result",
                        "public DataOwnershipV1 ABI remains scalar bool/i32 v1",
                    ));
                    module_indices.push(None);
                    continue;
                };
                module_indices.push(Some(exports.len()));
                exports.push(raw_abi::Export::new(
                    name.clone(),
                    raw_abi::Signature::new(parameters, result),
                ));
            } else {
                module_indices.push(None);
            }
        }
        indices.push(module_indices);
    }
    if let Ok(abi) = verify_v1(raw_abi::Module::new(exports)) {
        (Some(abi), indices)
    } else {
        errors.push(first_export_span.map_or_else(
            || {
                error(
                    "ZRYNA-I3009",
                    "public scalar ABI claims are invalid",
                    "use unique portable scalar ABI v1 exports",
                )
            },
            |span| {
                error_at(
                    "ZRYNA-I3009",
                    span,
                    "public scalar ABI claims are invalid",
                    "use unique portable scalar ABI v1 exports",
                )
            },
        ));
        (None, indices)
    }
}

fn abi_type(layouts: &VerifiedLayouts, id: raw::TypeId) -> Option<raw_abi::Type> {
    match layout_type(layouts, id)?.category() {
        TypeCategory::Bool => Some(raw_abi::Type::Bool),
        TypeCategory::I32 => Some(raw_abi::Type::I32),
        _ => None,
    }
}
fn layout_type(
    layouts: &VerifiedLayouts,
    id: raw::TypeId,
) -> Option<zryna_layout::VerifiedType<'_>> {
    layouts.types().nth(id.0 as usize)
}
fn check_span(span: Span, file: FileId, sources: &SourceMap, errors: &mut Errors) {
    if !sources.resolve(span).is_ok_and(|resolved| resolved.source().id() == file) {
        errors.push(error(
            "ZRYNA-I3004",
            "span is not owned by its containing module",
            "use an exact span from the module's final source file",
        ));
    }
}
fn same_logical_universe(a: &VerifiedLayouts, b: &VerifiedLayouts) -> bool {
    let mut left = a.types();
    let mut right = b.types();
    if left.len() != right.len() {
        return false;
    }
    left.all(|x| {
        right.next().is_some_and(|y| {
            x.id().index() == y.id().index()
                && x.category() == y.category()
                && x.nominal_identity() == y.nominal_identity()
                && x.fields()
                    .iter()
                    .map(|f| (f.ordinal(), f.ty().index()))
                    .eq(y.fields().iter().map(|f| (f.ordinal(), f.ty().index())))
                && x.variants()
                    .iter()
                    .map(|v| (v.ordinal(), v.payload().map(LayoutTypeId::index)))
                    .eq(y
                        .variants()
                        .iter()
                        .map(|v| (v.ordinal(), v.payload().map(LayoutTypeId::index))))
                && x.array_length() == y.array_length()
                && x.referenced_type().map(LayoutTypeId::index)
                    == y.referenced_type().map(LayoutTypeId::index)
        })
    })
}
fn aggregate_operand_count(kind: &raw::InstructionKind) -> usize {
    match kind {
        raw::InstructionKind::StructConstruct { fields, .. } => fields.len(),
        raw::InstructionKind::FixedArrayConstruct { elements, .. }
        | raw::InstructionKind::VecConstruct { elements, .. } => elements.len(),
        raw::InstructionKind::EnumConstruct { payload, .. } => usize::from(payload.is_some()),
        _ => 0,
    }
}
fn terminator_edge_count(kind: &raw::Terminator) -> usize {
    match kind {
        raw::Terminator::Return { .. } | raw::Terminator::Trap { .. } => 0,
        raw::Terminator::Jump(_) => 1,
        raw::Terminator::Branch { .. } | raw::Terminator::WeakUpgradeBranch { .. } => 2,
        raw::Terminator::EnumMatch { arms, .. } => arms.len(),
    }
}
fn checked_add(current: usize, extra: usize, label: &str, errors: &mut Errors) -> usize {
    current.checked_add(extra).unwrap_or_else(|| {
        errors.limit(label, usize::MAX);
        usize::MAX
    })
}
fn error(code: &'static str, message: impl Into<String>, guidance: &'static str) -> Diagnostic {
    Diagnostic::error(code, None, message, guidance)
}
fn error_at(
    code: &'static str,
    span: Span,
    message: impl Into<String>,
    guidance: &'static str,
) -> Diagnostic {
    Diagnostic::error_at(code, span, message, guidance)
}
fn error_with_optional_span(
    code: &'static str,
    span: Option<Span>,
    message: impl Into<String>,
    guidance: &'static str,
) -> Diagnostic {
    let message = message.into();
    match span {
        Some(span) => error_at(code, span, message, guidance),
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
        } else {
            self.diagnostics.push(error(
                "ZRYNA-I3202",
                format!(
                    "DataOwnershipV1 verification reached its diagnostic limit of {MAX_DIAGNOSTICS}"
                ),
                "fix retained diagnostics before verifying again",
            ));
            self.exhausted = true;
        }
    }
    fn limit(&mut self, label: &str, maximum: usize) {
        if !self.exhausted {
            self.diagnostics.push(error(
                "ZRYNA-I3201",
                format!("DataOwnershipV1 {label} exceeds its limit of {maximum}"),
                "reduce the program before IR verification",
            ));
            self.exhausted = true;
        }
    }
    fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }
    fn finish(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

mod borrow_index;
mod indexed_access;
mod indexed_binding;
use borrow_index::BorrowIndex;
pub use indexed_binding::VerifiedIndexedBinding;
mod indexed_borrows;
pub use indexed_access::VerifiedIndexedProjection;
pub use indexed_borrows::{
    VerifiedBorrowReferentDrop, VerifiedBorrowReplacement, VerifiedIndexedBorrow,
};

#[cfg(test)]
mod tests;
