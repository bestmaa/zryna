# Zryna semantics

Permanent Zryna-owned phase boundary from verified provider-neutral syntax to Universal IR. The
legacy M1 entry returns raw IR to its verifier call site; the isolated M2 entry seals IR internally.

`SemanticInput::try_new` accepts only a verified protocol-v2 snapshot bound to the exact immutable
`SourceMap` that issued it. It rejects every snapshot containing a provider error, so parse
recovery or unsupported syntax cannot enter name resolution, type checking, or lowering as a
smaller program. Provider warnings remain non-fatal and are preserved by `zryna-driver`.

The internal `definition_queries` module freezes exact function and parameter declaration spans
and parameter reference spans only after this protocol-v2 scalar checker succeeds. The driver's
`diagnostic_sessions` boundary retains that opaque, source-map-bound index for bounded definition
lookup. The separate language-server transport may invoke it through the driver; semantics adds no
general symbol coverage, transport logic, CLI route, or new language profile.

## First strict source subset (protocol v2/M1)

`lower` currently requires exactly one source file containing at least one explicitly exported
function. It accepts:

- portable scalar-ABI export names that are unique exactly and under ASCII case folding;
- explicitly annotated `i32` and `bool` parameters and results;
- unique parameter bindings and references to those parameters;
- exactly one value-returning statement per function;
- `i32` literals in the inclusive range `-2147483648` through `2147483647`;
- `bool` literals; and
- left-associative `+` expressions whose operands are both `i32`.

Missing annotations, `any`, unknown types, unresolved names, duplicate parameters or exports,
invalid or colliding export names, out-of-range literals, non-`i32` addition, mismatched return
types, empty entrypoints, multiple files, and bodies without exactly one return fail with bounded,
deterministically ordered `ZRYNA-M1xxx` diagnostics. Semantic input sizes are compile-time proven
not to exceed the corresponding Universal IR limits.

The protocol-v2 bootstrap path rejects parenthesized expressions, calls, local declarations,
control flow, and heap-backed expressions before M1 semantics instead of normalizing a smaller
program. Multi-file and module semantics remain outside this legacy `lower` gate, which rejects
multiple files. Protocol v3 and the separate M2 entry below do not inherit those M1 restrictions.

M1 semantic success returns raw `Program`; only `zryna-ir::verify` can turn it into backend-safe
`VerifiedProgram`. `bool` is valid source semantics and lowers to `BoolLiteral`, but the current
`I32V1` verifier deliberately rejects it with `ZRYNA-I1006`. Therefore the current complete
source-to-verified-IR success path is the documented `i32` subset. Enabling `bool` requires one
future universal profile implemented consistently by every active backend.

This crate owns language meaning and must never depend on a replaceable frontend provider.

## Isolated native C source, declaration and body boundaries

`native_c_v0::verify` consumes the bounded raw sidecar wire, the original immutable
`SourceMap`, separately authenticated restricted foreign syntax and explicit
captured library header/policy bytes. It independently computes source, header,
policy and domain-separated declaration SHA-256, compares exact canonical
policy projections, checks identities/signatures/allocator/status relations and
replays every parsed intrinsic and export binding. It returns an opaque
declaration-only set, retaining distinct C-int and C-i32 spellings. Temporary
scalar ABI projections reuse defensive export-name validation only.

The separate `native_c_v0::body::verify_bodies` consumes that opaque declaration
authority and its exact original map. It checks every retained complete function,
source type and local binding; tracks immutable status aliases to the exact call;
checks status-zero output dominance, paired takes, nominal allocator/release
identity and linear owner moves; and rejects foreign tokens across even private
function boundaries. Its opaque `VerifiedForeignBodies` retains the original
declaration/material authority and complete source occupants alongside read-only
typed expressions and conditional foreign flow steps. Views cannot construct a
body authority. Scalar exports stay total, with wrapping i32 addition.

Each call records a pre-effect reservation against the execution-instance limit
of 64 live/reserved obligations, rather than rejecting 65 lifetime acquisitions
statically. Successful non-null acquisitions belong to the exact call and resource
group before validation or take; untaken outputs remain in conditional reverse
cleanup. Unknown statuses and malformed outputs require host/ABI failure, while
release failure overrides completion with unresolved cleanup and no retry. Safe
empty-byte release skips C entry while still consuming its token.

These steps require future runtime length/range/UTF-8/count/Boolean checks,
freshness/output validation, shared acquisition accounting, exact private M3
preparation failure identities and physical cleanup. They do not demonstrate
those actions occurred. Library pointer/freshness/failure promises remain trusted
C contracts. IR/MIR construction, object inspection, linking and public activation
remain separate requirements; existing M3 authorities accept none of these foreign
forms. See the [source/declaration captures](../../tests/native-c-auth-v0/README.md)
and [body entry and tests](src/native_c_v0/body/mod.rs).

The isolated `native_c_v0::body::compose_private_boundaries` consumes only those
complete opaque bodies and their original map. Existing independent Linear32 and
Linux x86-64 layout verification, followed by mandatory ownership-runtime ABI
verification, issues its retained private layout and status/trap declarations.
Its internal candidate replay checks complete source origins, private moves,
scoped retention, preparation stages and mixed reverse completion order. Cloned
requirement records cannot construct `VerifiedPrivateBoundaries`.

`Vec<i32>` loans require length/range checks and separate one-byte packed scratch,
with exact private Allocate/Release requirements; String loans retain initialized
UTF-8 storage without inventing an allocation. Returned foreign bytes require a
distinct private Vec allocation, checked four-byte element capacity, unsigned byte
expansion, initialized-prefix completion and length commit. Allocation/capacity
faults retain issued private traps and every prior owner. Private allocations do
not charge the unchanged global foreign budget of 64 live/reserved owners.

Every exit ends its scoped loans before conditional reverse drops. A prepared
result remains protected until all required releases confirm success; a release
defect overrides return/error/trap without retry and leaves the result plus the
remaining obligations unresolved. Process failure promises no physical cleanup.
This boundary authenticates static requirements, including private entry storage
obligations; it does not execute conversions, allocate memory, validate an actual
caller entry, or extend legacy protocol-v4/M3/IR/MIR/backend/driver/public seals.
See the [private composer](src/native_c_v0/body/private_boundary/mod.rs) and
[independent private boundary tests](src/native_c_v0/body/tests/private_boundary/mod.rs).

## Internal M2 semantics boundary

The separate `control_flow_v1` module consumes only an exact source-map-bound verified
protocol-v3 snapshot and an entry present in that snapshot,
revalidates the complete deterministic module graph, owns module/callable/lexical names and exact
types, and lowers the frozen M2 semantic subset. It implements `i32` arithmetic and signed
comparisons, Boolean equality, initialized locals, assignment, lexical shadowing, and statically
resolved acyclic direct calls while preserving left-to-right once-only evaluation.

M2 semantic success returns only mandatory-verifier-approved
`zryna_ir::control_flow_v1::VerifiedProgram`; raw M2 IR never leaves the boundary. Canonical `if`
and `while` lowering carries definite mutable state through typed merge and loop-header parameters,
with reachability and all-path return checks. No backend or public CLI selects this profile. See
[M2 straight-line semantics](../../docs/M2_STRAIGHT_LINE_SEMANTICS.md) and
[M2 control-flow semantics](../../docs/M2_CONTROL_FLOW_SEMANTICS.md).

## Internal M3 aggregate and owned-data foundation boundary

The separate `data_ownership_v1` module consumes an exact source-map-bound verified protocol-v4
snapshot and entry file. It owns nominal identities, names, exact types, field and variant
ordinals, and recursively Copy classification. It verifies the same semantic type graph through
both `Linear32V1` and `LinuxX8664V1` layout authorities and lowers Copy structs, enums, and fixed
arrays. The sealed semantic `VerifiedProgram` retains both mandatory-verifier-approved
`zryna_ir::data_ownership_v1::VerifiedProgram` and the exact verified ownership-runtime ABI
declaration authority; neither raw IR nor raw runtime declarations can be recovered.

Its structured owned route now composes lexical Block, If, While, WeakUpgrade and admitted Match
occupants across nested/repeated flow, early return, loop backedges and post-loop continuation.
Joins and backedges require exact ownership state; lexical shadowing restores the nearest outer
binding; fallible occupants retain verified reverse-prefix cleanup. The checked
[#271 closure matrix](../../docs/M3_STRUCTURED_OWNED_CONTROL_FLOW_MATRIX.md) binds every claimed
source, hostile-IR and block/edge resource row. This is compiler-only lowering into mandatory
verified IR, not runtime execution or public profile support.

Fixed-array access in this internal gate is limited to a compile-time in-range constant. The
completed internal Issue #81 boundary recognizes canonical String and `Vec<T>` type graphs and lowers
bounded private functions. String supports UTF-8 literals through
`StringFromUtf8`, explicit clone, checked concatenation, local moves, return with reverse-order
cleanup, and mutable root-local replacement. Vec supports construction, explicit clone for exact
`Vec<bool>`, `Vec<i32>`, and `Vec<String>`, local moves, return, push, checked indexing that returns
a Copy element, and replacement for the supported exact Vec roots.
Both routes admit private zero-argument producers and one-argument owned identity calls. They also
lower one top-level no-phi `if`/`else` from a bool literal or Copy bool parameter into canonical
entry/then/else/join blocks. Branch-local owned roots drop once in reverse order; incoming owners
must be restored exactly, and mutation of an incoming Vec is rejected before lowering its value.
Private String and exact Vec result functions additionally admit one terminal `if`/`else` whose
canonical graph has exactly three entry/then/else blocks. Each arm directly returns its owned-producing
expression with no join block or parameter, and cleanup excludes that returned owner.
Both routes also admit one top-level no-carried-owner `while` after supported declarations and
before the sole final return. Its condition is emitted in the canonical header, iteration-local
owners drop in reverse before the backedge, and both backedge and false exit restore the exact
incoming owner and String-byte state. The stable-place subset replaces one mutable outer String
after complete RHS preparation or pushes one Copy element into a mutable outer exact Vec; both
retain the outer place identity and use no owned loop-header phi. Vec loop replacement and
owned-element Vec loop push remain excluded.
Vec construction, push, and direct calls reserve their parent cleanup and result resources before
fallible child evaluation, using expression-aware net owner growth at exact limits.
The lowerer uses `InitializePlace`, `MoveFromPlace`, and the infallible `ReplacePlace` commit after
right-hand-side preparation. A separate bounded route constructs, moves, explicitly clones,
returns, and drops
acyclic owned Struct and FixedArray graphs with bool, i32, or String leaves, and owned Enums with a
payloadless variant or one supported Copy, String, Struct, or FixedArray payload. Structural clone
retains its source, creates a distinct result owner, derives the exact fallible String-leaf count
and root-enum active variant from sealed authorities, and reverse-drops only the initialized result
prefix on element failure. Mutable whole-root assignment for the same aggregate graphs prepares a
distinct right-hand side before the infallible `ReplacePlace` commit, rejects direct
self-consumption, and exposes the old destination's recursive drop shape. Constructor
operands are evaluated in sealed declaration or index order, whole-value moves transfer one owner,
and return cleanup drops surviving roots in reverse successful-completion order. Canonical static
struct-field and constant fixed-array projections additionally admit Copy reads, exact String-leaf
moves, and at most one exact supported Struct/FixedArray subobject move into a directly initialized
same-type local, at most one exact supported Struct/FixedArray subobject move into the final
same-type return of a parameter-free local-root function, at most one explicit clone of an initialized available non-Copy
Struct/FixedArray projection into the immediately following exact same-type local, plus
prepare-before-commit assignment to one mutable available String leaf. Projected aggregate clone
retains the enclosing root and its partial-state mask, uses the same layout-derived recursive
String-leaf failure cleanup as whole-root aggregate clone, and creates a distinct temporary owner.
The direct-local and final-return subobject routes materialize complete static descendant topology
before the move; the enclosing root keeps one masked cleanup obligation while the new local or
returned temporary owns the moved subtree. Final return preflights its cleanup plan and all pending
survivor actions before source materialization or masking. The projected-assignment gate also admits one complete supported static subobject move or
explicit clone between distinct local roots. Its exact producer/consumer shape is an immediate
`MoveFromPlace` or `ClonePlace` into one sole-use same-type temporary followed by `ReplacePlace`.
Move materializes and masks the complete source subtree. Clone retains the source without
materialized descendants and derives recursive preparation failure from sealed layout. Commit
recursively drops only the old target subtree, retains both roots and sibling masks, and preserves
the pending-root order.
One additional canonical private route extracts the complete non-Copy Struct or FixedArray payload
of a single-variant enum through an exhaustive one-arm `match`, initializes one exact direct local,
drops the now-empty enum root, and returns that local through a zero-argument continuation. The
payload source topology is complete, while the whole destination owner needs no duplicated
descendant places.
Assignment preparation retains the enclosing root and commit drops only the exact old leaf while
leaving sibling masks unchanged. A moved leaf is excluded from the enclosing root's recursive return cleanup;
disjoint leaves stay available, while a repeated or overlapping move and later whole-root
consumption are rejected. The private
String route reports use-after-move as `ZRYNA-M3011`; the bounded owned aggregate and enum route
reports it as `ZRYNA-M3014`; unresolved binding names report `ZRYNA-M3002`. Excluded private String shapes report
`ZRYNA-M3012`, and cumulative String-literal bytes are checked against the exact 8 MiB limit before
lowering.

M3 now completes the #272 internal owned-call boundary: explicit relative `.zry` aliases resolve
only within the authenticated acyclic closure and retain the callee's canonical `FunctionId`.
Dependency exports remain internal calls; only entry exports are public ABI candidates. Imported
signatures admit the complete sealed by-value ownership graph, including containers, aggregates,
handles and finite indirection. Foreign nominal identity is preserved without adding type-import
syntax; borrowed imports, owned public ABI, indirect calls and recursion remain excluded. The
checked [owned-call closure matrix](../../docs/M3_OWNED_CALL_CLOSURE_MATRIX.md) binds source,
structured-CFG, hostile-IR and resource evidence.

M3 also completes the #273 exhaustive enum-match boundary as a checked compiler-only closure
candidate. Authenticated matches evaluate one scrutinee, cover each sealed variant exactly once,
expose only the active payload and join exact Copy or owned results through the structured CFG.
The [complete enum matching matrix](../../docs/M3_COMPLETE_ENUM_MATCHING_MATRIX.md) binds source,
independent-IR and resource/overflow evidence. Terminating or wildcard arms and inactive payload
access remain excluded from that matrix.

The completed #275 compiler boundary adds non-indexed shared/exclusive borrowing for admitted
owned roots, static Struct/FixedArray projections and refined active enum payloads, including
nested lexical/direct-call use, exact cleanup and deterministic hostile/resource rejection. The
[checked borrowing matrix](../../docs/M3_NONINDEXED_OWNED_BORROWING_MATRIX.md) binds enabled source
and independent-IR evidence. Runtime/backend behavior and public activation remain excluded.

The separate #278 mixed-result function route also prepares and commits fully initialized mutable
mixed Struct/Enum/FixedArray/Vec root replacement. Its affine plan proves exact destination
retention, RHS owner transfer and final transition capacity before materializing the RHS. Repeated
assignments retain precise old-value cleanup and pending completion order. Legacy-shaped targets,
partial/projected replacement, generic clone, mixed call signatures and CFG remain unchanged.
See the [composition evidence](../../docs/M3_OWNERSHIP_COMPOSITION_EVIDENCE.md) for proof limits.

An internal test-only fault/drop-trace oracle covers every ABI-admitted failure of the implemented
String construction/clone/concat and Vec allocation/reserve operations, plus the separate verified
Vec bounds trap. It consumes authenticated status disposition/trap declarations, retains all
pre-commit operand owners, excludes the uncommitted result, and checks exact reverse cleanup,
deterministic replay, and bounded event accounting. It is not allocator execution, target runtime
fault injection, or a public interpreter. Exact `Vec<String>` clone additionally seals a distinct
element-failure cleanup that reverse-drops only the runtime-recorded initialized destination prefix
before pre-existing owners; this remains compiler evidence rather than allocator execution.
Supported aggregate clone uses its own `AggregateCloneElementFailure` role and typed initialized-
prefix action under the same rule; neither its fallible-leaf count nor root-enum active variant is
accepted from the fault injector.

The completed bounded boundary deliberately does not provide general owned lowering. General structural Vec clone beyond
String elements, nested aggregate clone graphs containing Enum, Vec, Shared, or Weak values,
aggregate-subobject moves outside one exact direct local, one parameter-free exact final return, or the one single-variant match-local enum
payload extraction, dynamic or Vec-element projections, projected aggregate clone outside its exact
direct-local or distinct-root static-projection forms, projected aggregate assignment outside one
complete-static-subobject-move-or-clone-or-whole-root-move-or-clone-to-static-projection site,
whole-partial-owner transfer outside
the exact-type direct-local,
final-return, or whole-root assignment Struct/FixedArray exceptions, general owned phi joins and
owned loop-carried phi joins remain unavailable. The #271 route separately admits repeated/nested
branches and loops, general scope-drop insertion, loop-body return and post-loop continuation;
`break` and `continue` remain excluded. The legacy String/Vec selectors retain their zero/one-
argument checkpoints, while the generic #272 route handles broader exact internal signatures.
The legacy owned aggregate route is parameter-free, private, and straight-line;
its projection subset is limited to static Struct/FixedArray Copy reads, String-leaf moves, one
direct-local supported Struct/FixedArray subobject move, one final-return supported
Struct/FixedArray subobject move, String-leaf clone, one direct-local
supported Struct/FixedArray clone, String-leaf assignment, and at most one private straight-line
assignment that moves or clones a complete available same-type static Struct/FixedArray subobject
between distinct local roots, or moves or explicitly clones a distinct fully initialized exact same-type
supported non-Copy Struct or FixedArray whole root into a mutable available
`StructField`/`FixedArrayConstant` projection. Subobject move consumes only its selected subtree and
masks it beneath the still-pending source root. Whole-root move consumes its source; clone retains
it and its prepare/prefix failure paths retain source and destination. Commit recursively drops the
exact old target. Projected-subobject move or clone retains both enclosing roots, pending-root order,
and sibling masks; projected clone also retains its source subtree and seals prefix failure cleanup;
whole-root clone retains source and destination; whole-root move retains only the
destination after consuming its source. A partially moved Struct or FixedArray root may move
through one exact direct-reference initializer into a same-type local, through one final exact
direct-reference return, or into one distinct mutable fully initialized same-type whole-root
assignment destination. Assignment prepares the source without touching the destination, then an
infallible `ReplacePlace` drops the old destination and installs the exact partial mask. Lowering
preserves the complete recursive static topology, migrates the moved mask through every
temporary/local owner, excludes a returned owner from survivor cleanup, and invalidates old
owners. Enum-payload moves outside that exact one-arm direct-local form, dynamic or Vec-element
moves, same-root/overlapping, partial or already-moved projected sources, fresh sources or
multi-site aggregate assignment, calls, direct projected-clone
returns, CFG transfer, or public functions remain outside the narrow subobject route. Admitted projected clones retain the
enclosing root and its partial-state masks while creating one distinct temporary owner.
The separate bounded-borrow producer admits private Copy roots, exact shared/exclusive authority,
one shared reborrow, canonical conditional/loop scopes, and static recursively Copy Struct-field
or constant fixed-array paths. Static siblings are disjoint; same/ancestor/descendant paths
overlap. The completed Issue #116 implementation separately admits one private
parameter-free whole non-Copy root, one const shared alias in one lexical block, and only String
clone/checked concat, exact `Vec<bool>`/`Vec<i32>` Copy indexing, or supported whole
Struct/root-Enum/fixed-array clone. The source remains initialized and is returned after lexical
end; owned read results receive distinct owners through existing clone/cleanup/fault authority.
This does not extend `BorrowRead` beyond Copy or transfer an owner through the alias. Dynamic or
owned projections, multiple aliases, mutation, moves, calls, general control flow, stored
references, shared or weak references, and public owned parameters or results remain unavailable.
Issue #116 passed independent verification and required merge gates.
The Issue #323 private straight-line evidence additionally constructs one nonempty finite recursive
Enum through `Vec<Node>` indirection, moves and root-replaces it, explicitly clones it and pushes a
prepared clone. Mandatory verified IR retains active variants, exact owners and a finite symbolic
clone frontier. This adds no by-value recursion, runtime/backend route or public profile.
The Pair scalar oracle
is observed only by a test evaluator over opaque verified views. Enum matching is limited to an
internal single-return function with scalar literal, parameter, or active-payload arms; this is not
a general expression-level match implementation. No runtime, backend, driver, CLI, or public
`data-ownership-v1` profile selects this module. See
[M3 Copy aggregate semantics](../../docs/M3_COPY_AGGREGATE_SEMANTICS.md).
