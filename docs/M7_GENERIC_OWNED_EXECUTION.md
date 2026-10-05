# Internal generic owned execution

This #416 continuation adds a separately sealed ownership successor to the preserved
[JS/Wasm/native Copy chain](M7_GENERIC_COPY_NATIVE.md). It executes whole String moves,
Option/Result construction and match, lexical loans, explicit String clone and exact cleanup.
It preserves the frozen Copy wire and emitter behavior, existing scalar ABI, runtime header,
dependencies, lockfiles and profile/driver entrypoints. Public generic support and full #416
acceptance remain unfinished.

## Source and authority

The internal source fixture includes `identity<T extends ZrynaValue>(value:T):T`, instantiated
with String and imported through an exact named module path. For example:

```ts
const item: String = "α";
const found: Option<String> = Option.some<String>(identity<String>(item));
const score: i32 = match(found, {
  "Option.none": () => 0,
  "Option.some": (value) => discard<String>(value),
});
```

Source v5 verification remains mandatory. The test reader creates raw DTOs without semantic
authority; frozen DTOs are then decoded and verified against exact source bytes. These fixtures
are not evidence of TypeScript/native provider parity.

`instantiation::owned_v2::discover` checks all original opaque bodies before closed discovery
and layout issuance. It returns an opaque `OwnedInstanceContext`; an ordinary `InstanceContext`
cannot enter its producer. The semantic producer returns raw claims only. Independent v2 wire
decoding and IR verification bind the exact source, entry, dual layouts, runtime declarations,
complete instance graph, original opaque affinity and owner/loan/drop plan. Only the resulting
`VerifiedOwnedProgram` enters the three emitters. See the exact
[wire and cleanup contract](../spec/ir/GENERIC_OWNED_WIRE_V2.md).

The finite structural-clone continuation admits `clone(namedOwner)` only for stored
`Option<String>` and `Result<String,String>`, alongside the existing stored String clone.
It retains the original owner and any surrounding shared loans. A temporary outer shared loan
selects the active variant; only its String payload is cloned into a fresh owner and rebuilt
with the same tag. None performs no runtime allocation. Child loans end before the join and
outer loan. Repeated clones have distinct allocation identities and leave the source reusable.
Nested structural clone, other payload types, opaque `T` without Clone capability, and source
`clone(Borrow<String>)` remain excluded. Moved or exclusively borrowed owners reject.

The compiler-generated payload leaf uses opcode 7 in the distinct private
[owned wire v3](../spec/ir/GENERIC_OWNED_WIRE_V3.md). Both v2 encoding and decoding reject it;
v2's existing opcode 6 still requires a stored String. V3 yields the same untrusted decoded
carrier and enters the existing mandatory source/IR verifier and native MIR seal. It creates
no new execution authority, runtime symbol, layout, scalar ABI or public entrypoint.

The current source lane admits immutable and mutable whole stored locals, scalar literals/addition, explicit calls,
String literals/clones, whole-value moves, Option/Result construction and exhaustive match,
shared/exclusive complete-place loans, borrowed payload matches, nested lexical blocks and nested
early returns. Whole reassignment completes its replacement before releasing the old live value;
a moved binding can be reinitialized. Copy mutable roots and their observations have separate
SSA identities, so a loan of an immutable observation does not freeze the mutable source place.
Replacing any currently borrowed root is rejected at its exact assignment target.
Only scalar signatures may be public exports. Opaque generic parameters remain affine;
specializing to a Copy type cannot legalize repeated moves or add a Clone capability.

Nominal declaration bodies, other containers, projections/partial initialization, mutable
loan bindings and branch-dependent replacement/phi state remain rejected. Borrowed payloads can pass to
appropriately typed private callees; further borrowed observation/mutation operations are not
implemented. These are acceptance gaps, not restrictions added to the accepted full contract.

Structured `if` now admits explicit or omitted `else`, nested/repeated branches, lexical
shadowing, branch-local cleanup and direct early returns. The condition is evaluated once and
must have exact original bool type. Both returning arms have direct Return terminators and no
join block. Each fallthrough arm closes its local scope before an empty edge to a no-phi join;
incoming place identities and complete availability/loan/parent states must agree exactly.
Moving an opaque owner on only one continuing path rejects before specialization, including
in unused templates. Moving it on both paths leaves it unavailable after the join. Retained
loans may remain identical across empty edges; no loan is transferred as an edge argument.
Branch-dependent assignment to an incoming binding still requires a separate phi proof and
rejects even when both replacements have the same type. No implicit drop or owner repair is
inserted to reconcile unequal incoming states. A returning path does not constrain the state
of the remaining continuing path.

## Loop-plan prerequisite

The private independent owned planner now replays reducible natural-loop graphs once. It
computes complete dominators independently, removes only dominating backedges from forward
scheduling, and retains each loop-header owner, loan and reverse-cleanup-order snapshot. Every
backedge must restore that exact state after its edge transfers; it receives no implicit drop,
clone or merge repair. Body-local owners and loans must finish before returning to the header.
Forward joins still require exact state agreement, and unreachable, irreducible, foreign-entry
and unknown-target graphs reject. Header retention and topology inventories contribute to the
existing aggregate owner-plan credit before allocation; inherited typed CFG and loop-nesting
checks remain mandatory.

The bounded source `while` successor now builds a repeated condition header and carries mutable
bool/i32 bindings through existing block parameters and exact edge operands. Condition expressions
may contain their own match CFG; evaluation and temporary cleanup execute on every visit, including
the final false visit. Conditions must preserve incoming owner and loan state. Continuing bodies
close lexical locals and loans before restoring exact incoming availability, owner identities and
loan parents. Only scalar Copy binding identities may change. Direct returns need no backedge;
the false path retains header values for post-loop continuation. Nested/repeated loops and lexical
shadowing use the same replay. Opaque originals are checked before substitution, including unused
functions; Copy specialization cannot hide an owner move. Borrowed mutable scalar roots, owner or
aggregate header replacement, branch-dependent replacement, break/continue and unstructured CFG
remain excluded. Existing wire, layout, runtime, backend and public-profile contracts are unchanged.

## Target execution

JavaScript keeps private typed enum values and opaque runtime handles. Its private binding is
`globalThis[Symbol.for('zryna.generic.ownership.runtime.v1')]`, with captured helpers
`stringFromUtf8Copy(bytes,length)`, `stringClone(handle)` and `stringRelease(handle)`.
Creation/clone return `{status,handle}`; release returns status. Calls propagate a failure only
after the exact sealed cleanup. Scalar wrappers validate arity/types and prevent runtime reentry.
This binding is internal trusted-runtime plumbing, not a new public host grant.

Core Wasm uses private i32 carriers: String pointer/length/capacity and enum discriminant plus
maximum payload width. Complete result lanes are initialized; inactive payload lanes are never
interpreted or released. Private return globals are read only after successful calls. The
module imports exactly three runtime functions and one fixed 64 MiB memory under namespace
`zryna.generic.ownership.runtime.v1`. Private record frames occupy bytes 0..4095, source literals
start at 4096 and must end by 32 MiB; the proof allocator uses the upper half. Trusted runtime
implementations must reserve this region and obey the retained declarations. No memory or
aggregate value is publicly exported. Final bytes receive independent Wasm-1 validation and
an exact section/import/type/export/instruction/literal audit before artifact issuance.

Native lowering retains the exact owned seal. Private i64 lanes carry Linux String records and
tagged payloads; public wrappers retain scalar i32 carriers. Caller-owned private result slots
and a separate runtime record slot transport results. Failed calls do not read unwritten output
records. Three unchanged ownership-runtime String symbols are selected from the retained
Linux declarations and exact repository C header. Pinned Cranelift imports are colocated with
the final executable, giving direct E8/PLT32 relocations; the linker must reject an out-of-range
runtime. This linkage assumption does not qualify an arbitrary external runtime placement.

Native MIR bounds represented types at 256 lanes, private signatures at 256 parameters,
complete function storage at 65,536 lanes and code-generation work at 1,000,000 charged units.
String literal materialization and recursive cleanup contribute to those work credits. The
unchanged 8 MiB object ceiling and closed ELF audit check target, exact sections/flags/symbols,
body extents, three runtime imports, and independently derived caller/callee relocation counts.
The inventory utility grants no artifact authority and does not authenticate arbitrary machine
instructions. Only emission constructs the validated native artifact.

Source-controlled runtime failure cleans all current owners before propagation; return excludes
the transferred result. Release must not allocate or fail as a source trap. Host ABI violations,
host exceptions, memory corruption, process termination and engine failure have no controlled
cleanup promise. The direct harness currently pins allocation failure, not every inherited
trap identity or all runtime status transitions. Full runtime conformance remains required.

## Checked execution and remaining acceptance

Run with pinned tools, an isolated target and at most two jobs/test threads:
`python3 tests/m7-generic-owned-native/run.py <evidence-directory>`.
The Linux-only runner emits JavaScript, Wasm and ELF from the same verified program for each
single-module and imported fixture. Independent fixed oracles cover both Option variants,
both Result variants, active payload transfers, Unicode bytes, shared/exclusive payload loans including nested borrowed matches,
distinct cloned owners and lexical loan ending before consumption.

The native C runtime actually allocates, copies UTF-8, zeros and frees memory. Strict identity
records reject double frees and leaks. Each fixture has thirteen successful calls and four
isolated controlled failures: allocation sites 1/2/3 and clone failure. Child output proves
exact cleanup before SIGILL; the parent confirms the next valid call. JavaScript has 24 and
Wasm 17 fixed observations per fixture, with strict allocation/release traces and retry checks.
Hostile wire/plan/operation, final Wasm and ELF mutations fail independently and retain a
pristine recovery control. Older Copy artifact SHA assertions remain separate compatibility
oracles. This runner is direct internal evidence, not a registered supported-platform gate.

The separate bounded mutable-state runner is
`python3 tests/m7-generic-owned-cfg/run.py <evidence-directory>`.
Its single-module and imported fixtures use the same sealed program in all three emitters.
Fixed traces cover successful String replacement, replacement failure with the old owner still
live, moved opaque-local reinitialization, nested Option/Result replacement, self-move, Copy
observation preservation, distinct borrowed Copy aliases and nested return ending a live loan
before cleanup. Each fixture executes twelve native successes and five cleanup-before-SIGILL
failures; JavaScript and Wasm each execute seventeen fixed observations. The old runner and
its frozen fixtures remain separate. These whole-local operations do not establish conditional
joins, projection assignment or partial-state support. Loop evidence is separate below.

The separate bounded branch runner is
`python3 tests/m7-generic-owned-branches/run.py <evidence-directory>`.
Its independently read, frozen single-module and imported DTOs enter the same owned seal and
all three emitters. Fixed oracles cover both selected generic returns, nested Option/Result
branch-local cleanup, lexical shadowing, repeated/omitted else, retained shared loans,
asymmetric early return, and consumption on both continuing arms. Each module form checks
fourteen native successes and eight cleanup-before-SIGILL failures; JavaScript and Wasm each
check twenty-two observations, including failure and pristine retry. Independent raw branch,
edge, condition, scope-drop and failure-plan mutations reject; a separate hand-authored plan
test rejects unequal owner states in both successor orders without source-producer assistance.
Source replay charges complete branch snapshots, including copied binding type-key bytes,
before allocation, capped at the existing
1,048,576-unit owner-plan ceiling per original and across all closed specializations. Genuine
source checks the exact/first-extra branch boundary; synthetic credit tests retain checked
overflow, failed-state preservation and recovery. These are internal execution proofs, not
supported-platform, provider-parity or public profile admission.

The separate bounded loop runner is
`python3 tests/m7-generic-owned-while/run.py <evidence-directory>`.
Frozen independently read single-module and imported DTOs enter the same seal and all three
unchanged emitters. Fixed allocation identities, UTF8 bytes, release order and scalar results
prove zero/two iterations, repeated/final-false condition calls, a matched condition, nested and
sequential loops, bool/i32 header transfers, Option/Result lexical cleanup, stable incoming loans,
opaque generic ownership and asymmetric early returns. Each module form executes 32 native
successes and 24 allocation failures with cleanup before SIGILL; JavaScript and Wasm each execute
56 observations, including pristine retries. Independent backedge arity/value/type/target,
condition-edge, EndLoan and failure-plan mutations reject. Source witnesses retain hostile keyword,
newline and Unicode authentication. The existing 1,048,576 state-unit ceiling charges full retained
snapshots/type keys and scalar parameter/index/operand inventories before allocation. Genuine
source proves 435/436 loops under complete state credit and 256/257 scalar header places, then recovery.
These are private Linux execution proofs; they do not establish Windows generic execution or
full supported-platform/runtime qualification.

Full acceptance still requires nominal/container ownership and partial state, branch-dependent
replacement/phi state and further loop-carried owner/aggregate state, remaining borrowed operations, canonical multi-error diagnostics, full status/trap/runtime
qualification, provider parity, and supported-platform execution. This revision's Windows
validation occurs separately; older hosted Windows receipts do not qualify it. Driver/profile
activation remains a distinct future decision. Keep historical plain-cloud N4009 cleanup
failures separate from scoped init-style gates and record each exact tested revision.

The structural-clone runner is
`python3 tests/m7-generic-owned-clone/run.py <evidence-directory>`.
Independent frozen DTOs and allocation/release oracles cover all four variants, Unicode and
empty payloads, repeated clones, source retention, surrounding shared loans, every reachable
allocation failure, reverse cleanup and pristine retry. In each single/imported module form,
JavaScript and Wasm check 56 observations; native checks 32 successes and 24 cleanup-before-SIGILL
failures. None allocates nothing and inactive payloads are never cloned. This is private Linux
execution evidence; Windows generic execution remains a separate unrun requirement.

Structural selection precharges full snapshots before copying lexical bindings, complete type
key bytes, availability, loans and loan parents. Values, blocks, arm and edge inventories retain
their existing credits. With 400 scalar locals, genuine Option source permits 140 repeated clones
(1,044,960 state units) and rejects 141 (1,054,962); Result permits 129 (1,044,900) and rejects 130
(1,056,120), then both recover. The existing aggregate state ceiling remains 1,048,576 per original
and across closed production. Independent typed and owner-plan attacks supplement source-bound
wire mutations. Unsupported-source diagnostic context remains an existing acceptance gap.

The original ownership boundary rejects unsupported container signatures, including unused
functions and nested Option/Result containers, before closed discovery or layout. Independent
IR typing also rejects any retained type outside the admitted scalar/String/Option/Result lane.
Nested owned Option/Result fixtures check recursive selected-payload cleanup, nested exclusive
payload loans and an allocation-free inactive payload; they do not qualify other containers.

Source production and independent source replay charge aggregate value, block, call, payload
operand and literal credit before allocating each operation. Closed String literals are limited
to 64KiB each and their combined bytes to the 32MiB wire lower bound; encoding still checks the
complete message overhead. Opaque original checking retains no literal bytes. A genuine source
under 800KiB demanding 65 distinct instances of eight 64KiB literals is rejected before copying
literal 513, rather than materializing every body before checking aggregate budgets.
