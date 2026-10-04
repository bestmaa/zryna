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

The current source lane admits immutable locals, scalar literals/addition, explicit calls,
String literals/clones, whole-value moves, Option/Result construction and exhaustive match,
shared/exclusive complete-place loans, borrowed payload matches and nested lexical blocks.
Only scalar signatures may be public exports. Opaque generic parameters remain affine;
specializing to a Copy type cannot legalize repeated moves or add a Clone capability.

Nominal declaration bodies, other containers, projections/partial initialization, mutable
state, loops/if CFG and nested early returns remain rejected. Borrowed payloads can pass to
appropriately typed private callees; further borrowed observation/mutation operations are not
implemented. These are acceptance gaps, not restrictions added to the accepted full contract.

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

Full acceptance still requires nominal/container ownership and partial state, general mutable
CFG, remaining borrowed operations, canonical multi-error diagnostics, full status/trap/runtime
qualification, provider parity, and supported-platform execution. This revision's Windows
validation occurs separately; older hosted Windows receipts do not qualify it. Driver/profile
activation remains a distinct future decision. Keep historical plain-cloud N4009 cleanup
failures separate from scoped init-style gates and record each exact tested revision.

The original ownership boundary rejects unsupported container signatures, including unused
functions and nested Option/Result containers, before closed discovery or layout. Independent
IR typing also rejects any retained type outside the admitted scalar/String/Option/Result lane.
Nested owned Option/Result fixtures check recursive selected-payload cleanup, nested exclusive
payload loans and an allocation-free inactive payload; they do not qualify other containers.

Source production and independent source replay charge aggregate value, block, call, payload
operand and literal credit before allocating each operation. Closed String literals are limited
to 64KiB each and their combined bytes to the 32MiB wire lower bound; encoding still checks the
complete message overhead. Opaque original checking retains no literal bytes. A genuine source
under 300KiB demanding 513 distinct instances of one 64KiB literal is rejected before copying the
first excess instance, rather than materializing every body before checking aggregate budgets.
