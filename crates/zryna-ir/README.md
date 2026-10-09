# Zryna Universal IR

This crate owns the mandatory trust boundary between Zryna semantics and every output backend.
`Program`, `Function`, and `Expr` are untrusted claims. Only `verify(program, sources)` can create
a `VerifiedProgram`, and backends receive immutable `VerifiedFunction` views instead of the raw
program.

## Current universal profile

`UniversalProfile::I32V1` admits only `i32` parameters, results, literals, and signed wrapping
addition. `bool` and `unit` exist as reserved type vocabulary but fail verification until every
backend in a future profile implements their specified representation and behavior.

Raw IR includes `BoolLiteral` so Zryna semantics can represent a valid source-level Boolean before
profile selection. The verifier checks that the literal claims `Type::Bool`, then the `I32V1`
profile rejects that type. This keeps semantic legality separate from universal backend support and
does not allow a Boolean operation to reach a current backend.

Each function is paired by declaration index with a sealed scalar ABI v1 export from `zryna-abi`.
That lower authority owns the bounded logical-name grammar, reserved names, exact and portable
collision checks, typed signature, and deterministic JavaScript, core WebAssembly, and Linux
x86-64 native target mappings. Backends receive those immutable views instead of sanitizing raw
names. The current Universal IR profile still rejects `bool`; specifying its ABI does not enable it
before every active backend implements it.

## Verified expression shape

Every function contains one canonical expression tree:

- the body references an expression in the same arena and has the declared result type;
- every operand is a distinct earlier arena entry;
- every arena entry has exactly one owner, counting the body as the root owner;
- the arena is exact left-to-right postorder, with no shared or orphan entries;
- expression depth is at most 128; and
- every expression span resolves through the exact compilation `SourceMap`.

The verifier and current JavaScript emitter are iterative, so hostile depth cannot consume the
host call stack.

## Resource contract

| Resource | Limit |
| --- | ---: |
| Functions per program | 16,384 |
| Parameters per function | 256 |
| Parameters per program | 262,144 |
| Expressions per function | 16,384 |
| Expressions per program | 262,144 |
| Expression depth | 128 |
| Logical export bytes | 128 |
| Retained diagnostics, including the terminal diagnostic | 256 |

Resource limits are checked before proportional graph verification. Diagnostics are deterministic
and bounded: `ZRYNA-I1001`–`I1008` cover body, type, operation, and graph failures; `I1009`–`I1011`
cover logical exports; `I1201` reports a resource limit; and terminal `I1202` reports diagnostic
budget exhaustion or an internal bounded-construction failure.

Successful verification proves only this documented profile. Native lowering passes explicit raw
claims through the native MIR verifier, which retains the sealed ABI mapping before fixed-target
object codegen. Linux x86-64 object emission is implemented. Driver-owned linking and execution are
separate, capability-checked trust boundaries; neither is implied by `VerifiedProgram`.

## Isolated `ControlFlowV1` verifier component

The `control_flow_v1` module implements the separately versioned M2 Universal IR trust boundary.
This remains an internal compiler component rather than a directly callable public IR API. The
public driver reaches it only through exact `--profile control-flow-v1`, after authenticated module
closure and M2 semantics; omission preserves the unchanged M1 `VerifiedProgram` path. All three M2
backends consume only the opaque verified views described here.

`control_flow_v1::raw` can express sealed-module claims, entry exports, internal functions, dense
blocks and values, exact scalar operations, direct calls, block arguments, and explicit
return/jump/branch terminators. Its verifier independently binds the complete module inventory and
entry file to one exact final `SourceMap`; validates every span against its containing module; and
proves dense identity allocation, exact types, definition-before-use, dominance, reachability,
reducible loop shape, return completeness, acyclic calls, and all frozen graph budgets. Successor
and predecessor views are both derived from the exactly-one explicit terminator on each block, so
they cannot disagree through a separately trusted claim. Verification is iterative and diagnostics
are bounded.

Successful verification exposes only immutable module, function, block, instruction, terminator,
and sealed-identity views. It never exposes the retained raw program. Only entry-module functions
with explicit export claims enter scalar ABI v1; dependency and unexported functions remain
target-internal. Internal straight-line and control-flow semantics now lower into this boundary.
Deterministic internal JavaScript and direct core WebAssembly lowering are implemented. The
separate [M2 native MIR profile](../../docs/M2_NATIVE_MIR.md) now lowers these same sealed identities,
operations, calls, blocks, and edges and independently verifies the resulting target-specific
claims. Audited Linux x86-64 native object emission and typed link/run, explicit CLI activation,
deterministic manifest v2 publication, and fixed-oracle three-target conformance are implemented
through later verified boundaries; none weaken this IR constructor boundary.

| `ControlFlowV1` IR resource | Limit |
| --- | ---: |
| Modules | 4,096 |
| Functions per module / program | 4,096 / 16,384 |
| Parameters per function / program | 256 / 262,144 |
| Blocks per function / program | 4,096 / 65,536 |
| Block parameters | 256 |
| Values per function / program | 16,384 / 262,144 |
| CFG edges per function / program | 8,192 / 131,072 |
| Direct-call edges | 65,536 |
| Static call depth / loop nesting | 128 / 128 |
| Retained diagnostics including terminal diagnostic | 256 |

`ZRYNA-I2xxx` identifies structured-IR failures. `ZRYNA-I2201` is deterministic resource
exhaustion and `ZRYNA-I2202` is terminal diagnostic-budget exhaustion or an impossible bounded
construction failure.

## Isolated `DataOwnershipV1` verifier component

The independent hostile structured-CFG evidence for Issue #271 is indexed by the
[structured owned control-flow matrix](../../docs/M3_STRUCTURED_OWNED_CONTROL_FLOW_MATRIX.md).
It pins exact ownership, initialization, projection masks, active variants, edge arguments,
loop-header state, result owners, return/trap cleanup, diagnostic order and spans. Successful
verification exposes an immutable compiler trace; it does not execute cleanup, calls, upgrades,
allocation, refcount operations, or target code.

The `data_ownership_v1` module is the separate internal M3 Universal IR trust boundary. It does
not extend the M1 `I32V1` program or the M2 `ControlFlowV1` program. Its verifier accepts raw M3
claims only with the independently supplied final `SourceMap`, expected entry file, and verified
`Linear32V1` and `LinuxX8664V1` layout snapshots. It proves that the source-map identity,
target-neutral type universe, exact storage targets, and both claimed layout fingerprints agree,
then retains both opaque layout authorities in the verified program.

Raw programs contain their own dense module, function, block, value, place, borrow, and cleanup
arenas. Ownership transitions are derived from instructions and CFG edges rather than accepted as
an independent raw state graph. Verification checks exact scalar ABI exports, typed aggregate operations,
place projections, ownership state, nonescaping borrows, control-flow joins, and deterministic
cleanup before exposing immutable views. The retained runtime value is only the closed
`OwnershipRuntimeV1` contract identity. It is not the sealed runtime ABI authority or a runtime
implementation. The separate issue #80 authority verifies declarations, authenticated layouts,
header evidence, and pure transitions; no helper implementation, target runtime, backend, driver
route, CLI profile, or public aggregate ABI is supplied here.

`BorrowRead` remains restricted to an exact Copy referent and result. The current semantic
owned-root shared-read checkpoint does not add a non-Copy borrow transfer instruction: it brackets
existing verified String clone/concat, Vec Copy-index, or supported aggregate-clone instructions
with one shared `BeginBorrow`/`EndBorrow`. Existing verifier rules keep the source owner pending,
require distinct owned result places where applicable, and retain the existing site-bound
cleanup/fault authority.

The indexed-borrow prerequisite binds a FixedArray/Vec element to an evaluated i32 index while
retaining the complete container as its conservative conflict region. Bounds failure is a sealed
pre-begin cleanup site, not a target-selected trap. `BorrowReplace` supplies exact non-Copy
replacement through exclusive authority without creating a dynamic element owner place or dropping
the conflict container. Copy-only `BorrowRead`/`BorrowWrite` are unchanged. See the
[indexed authority contract](../../docs/M3_INDEXED_BORROW_AUTHORITY.md) for evaluation, mutation,
cleanup, call and resource rules. This does not activate the dependent source producers or targets.

The Issue #81 verifier foundations additionally admit bounded immutable UTF-8 bytes through
`StringFromUtf8`, derive exactly one non-Copy owner place per owned value, preserve Copy
parameter/local/temporary storage without adding Copy cleanup obligations, and transfer pending
drop order across calls, returns, and CFG edges. `ReplacePlace` is an infallible commit and carries
no prepare-failure cleanup identity. Its verified view derives the old destination's exact
recursive drop traversal from the pre-commit state for either a canonical root or a static
projection, and ownership replay transfers the prepared source subtree's state and active enum
variants without changing enclosing siblings. The current semantic producer uses that instruction
for private root-local String, supported Vec, and supported whole-root aggregate replacement;
projected, call, and CFG replacement are not part of the current semantic checkpoint.

`VecClone` currently admits only exact `Vec<bool>`, `Vec<i32>`, and `Vec<String>` sources, preserves
the source owner, and requires one distinct temporary result owner. Every clone binds allocation
failure to its exact prepare cleanup. `Vec<String>` additionally binds per-element `StringClone`
failure to a distinct sealed role whose first typed action reverse-drops the runtime-recorded
initialized destination prefix before pre-existing roots. Executable consumers must use the typed
verified drop-action view for this role rather than treating its root-only compatibility view as an
ordinary whole-place drop. This does not claim general `Vec<T: Clone>` support.

`ClonePlace` additionally admits supported non-Copy Struct, FixedArray, and root Enum graphs whose
fallible leaves are Strings. It preserves the source, produces one distinct result owner, and binds
String-leaf failure to a separately site-bound `AggregateCloneElementFailure` plan. That plan starts
with the typed `AggregateInitializedPrefix` action and then lists every pre-existing live root in
reverse order. The verifier derives the fallible-leaf count from retained Linear32 layout and the
root Enum's active variant from source ownership state; neither fact is accepted from a caller.
One non-root `ClonePlace` exception is independently sealed to at most one initialized non-Copy
Struct or FixedArray source reached only by `StructField`/`FixedArrayConstant`, in one private
straight-line function, immediately followed by exact same-type `InitializePlace` into a root
local. The result has one unique temporary owner and one use. Root clones remain unchanged;
Enum-payload, public, CFG, direct-return, alternate-use, and second projected-clone contexts fail
closed.
One static Struct/FixedArray projection move may instead be the final instruction and sole returned
value of a parameter-free private straight-line function. Its source must have one local root and
complete exact descendant topology, its result must have one unique temporary owner and one use,
and return cleanup must mask the complete source subtree while excluding the returned owner.
Parameters, public/CFG contexts, alternate consumers, missing topology, and second projected-move
sites fail closed.
One projected aggregate `ReplacePlace` exception is independently sealed to at most one combined
private straight-line site. The target is a `StructField` or `FixedArrayConstant` path rooted in a
local. The immediately preceding producer may be `MoveFromPlace` for one complete same-type static
Struct/FixedArray subobject rooted in a distinct local, `MoveFromPlace` for one distinct fully
initialized same-type whole local, or explicit root `ClonePlace`, which retains that whole-root
source and carries exact prepare and initialized-prefix cleanup. Every form uses one unique typed
temporary exactly once, immediately at `ReplacePlace`. Projected move requires the source's exact
static descendant topology; replay masks that whole source subtree beneath its still-pending root.
Commit recursively drops only the exact old target subtree. A projected-subobject clone uses the
same immediate sole-use replacement shape, retains its source without requiring descendant place
topology, and authenticates layout-derived prepare and initialized-prefix failure cleanup. Projected
subobject move or clone retains both local roots, pending order, and sibling masks; whole-root clone
retains source and destination; whole-root move consumes its source and retains the destination.
Same-root or overlapping paths, incomplete/partial/moved projected sources, Enum/Vec/dynamic paths,
public or CFG use, alternate ordering/use, and second sites fail closed.
Nested Enum, Vec, Shared and Weak graphs remain outside this projected-transfer checkpoint. The
separate generic clone authority admits finite non-handle values whose type recursion crosses Vec
indirection and visits each reachable type identity once; it does not admit by-value cycles or
claim runtime traversal.

Every raw cleanup plan is bound to exactly one verified site and one closed role:
`PrepareFailure`, `VecCloneElementFailure`, `AggregateCloneElementFailure`, `CallTrap`, `Return`, or
`ControlledTrap`. A cleanup-bearing site with a foreign
identity, an orphan plan, or a plan reused by another site fails closed. Opaque verified site and
drop-action views expose that authority without returning raw plans; explicit `DropPlace` views
derive their recursive state from the instruction's pre-drop program point. These are compiler
proof foundations only. The verifier vocabulary is broader than the current private straight-line
String/Vec/aggregate semantic producer; general projections, calls and CFG ownership, target cleanup,
allocators, and executable M3 profiles remain unavailable.

The exact authority tuple, raw vocabulary, limits, diagnostics, and verified-view contract are
documented in [`M3_DATA_OWNERSHIP_IR.md`](../../docs/M3_DATA_OWNERSHIP_IR.md).

## Command H1 compiler candidate

`command_h1_v1::verify` separately admits complete command source, the sole exported
`main(): bool`, and at most one source-authenticated literal-key environment operation.
The reserved `EnvLookupV1` is a closed owned enum with `Found(String)` and `Missing`;
source and IR exhaustive matches must agree. Ordinary `data_ownership_v1::verify` rejects
the environment opcode and the distinct `CommandH1V1` runtime identity.

The command wrapper exposes immutable owned-body views without an ordinary M3 program accessor.
Its issuing identity is process-local and preserved by clones; it is not an artifact hash or grant.
Its closed memory witness specifies 256 pages with static bytes below 65536, language bytes below
15728640, and a final 1 MiB canonical arena. This witness does not implement an allocator.
Source declarations retain the 4096 limit; command mode reserves exactly one additional nominal
slot when its builtin outcome is used. Other ownership/CFG/type/cleanup limits remain enforced.

This is the local compiler candidate for the
[unaccepted runtime contract](../../docs/WASI_COMMAND_H1_CONTRACT_DRAFT.md).
It does not instantiate a host, approve grants, execute WASI, or activate public CLI support.
