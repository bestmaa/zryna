# Native C v0 declaration and acceptance review

Status: **specified-only normative future contract**, for
[#364](https://github.com/zryna/zryna/issues/364), effective upon normal integration.
This completes the specification surface of the [ABI contract](NATIVE_C_INTEROP_V0.md)
and [operation policies](NATIVE_C_INTEROP_V0_ACCEPTANCE.md), with the exact
[sidecar/source contract](NATIVE_C_INTEROP_V0_SOURCE.md). It records specification
acceptance through integration, not historical sign-off or executed conformance.
The fixed records below are inert reference inputs, not current compiler
inputs. No syntax protocol, IR, MIR, selector, manifest or runtime is activated.

## Acceptance matrix

| #364 requirement | Complete specification input | Remaining implementation or execution |
| --- | --- | --- |
| Exact Linux type widths, alignment and calls | ABI table; pinned primary psABI comparison; inert C11 header checks | Execute the fixed carrier/ABI tuple's calling-sequence and generated-header proof |
| Every operation's library, allocator, owner, lifetime, cleanup and failure | Eight exact header operations in the acceptance table; closed declaration vocabulary and failure rules below | Implement the fixed source projection and wrapper primitives; independently verify declarations and raw IR/MIR |
| Positive/negative scalar, buffer, handle and reverse-client cases | Acceptance table plus fixed [review vectors](../../tests/native-c-abi-v0/review-vectors.json), including ambiguity and limits | Current tests check design consistency only; #417 executes the whole matrix |
| Private M3 layouts; raw versus safe obligations | ABI contract; import/export layouts and exact authority chain below | Accepted layout/ownership authorities are prerequisites for the corresponding implementation, not evidence of FFI |
| Design/prototype/implementation/conformance/activation split | ABI stage table; fixed decision and delivery table below | Integrate this specified-only contract; each later platform and activation needs separate evidence |

The accepted #357 composition decision is available. #361's source-only decision
and #362's trust decision are available; their native extensions remain conditional
on this #364 contract's normal integration. Neither supplies implemented FFI merely because its
issue is closed. #170 and native parser replacement are not prerequisites.

## Closed declaration vocabulary

Use an independently supplied, source-bound foreign declaration, not C-header
parsing as semantic authority. The closed schema freezes exact field names and
types; the sidecar contains the following semantic inputs:

| Field | Fixed value and validation |
| --- | --- |
| contract | `zryna-native-c-interop-v0`, version `0` |
| target / ABI / convention | Exact tuple in the identity section; no host-dependent inference |
| direction | `import` or `export`; neither implies a public selector |
| library | Exact library-contract identity for imports; exact output-module identity for exports |
| logicalName / symbol | A checked logical identifier and exact C symbol; exports derive their prefix, imports never derive library filenames |
| parameters / result | Ordered parameter records and one result; exact ABI spellings and roles below |
| resources | Ordered buffer/handle policies, each naming parameter/result slots by index |
| outcomes | Exact successful/recoverable statuses, initialized outputs and failure atomicity; controlled traps remain distinct |
| execution | `synchronous`, `invoking-thread`, `no-retention`, `no-callback`, `no-reentry`, `no-concurrent-entry`, `no-unwind` |
| sourceBinding | Portable ASCII path, exact source-byte SHA-256, declaration/call byte span and ordinal, authenticated against the final SourceMap; no claimed hash alone |

Unknown fields/tags, null placeholders, missing policies, duplicate names/slots,
unknown versions and inconsistent references reject the entire declaration set.
Canonical source identity is the authenticated module/path and exact source span;
the record ordinal is its zero-based canonical operation-array index, not an AST
node identity. Discovery order, local alias and provider node IDs cannot create a
different library or signature. Same-header typedef compatibility does not erase
the recorded distinction between `c-int` and `c-i32`.

Both logical names and admitted external symbols use ASCII
`[A-Za-z_][A-Za-z0-9_]*`; no symbol versions, assembler aliases or decoration is
inferred. Logical exports retain scalar ABI v1's keyword/defensive-name rejection.
Exact and ASCII-case-folded collisions reject across the complete linked symbol
set. With the 13-byte `zryna_c_v0_e_` prefix and 128-byte symbol ceiling, an export
logical name is limited to 115 bytes; imported logical names may use 128 bytes.

The following is the closed ABI spelling vocabulary. It is notation for
review records, **not Zryna type syntax** or permission to infer a type from a name.

| Review spelling | Exact C type / role | Admission |
| --- | --- | --- |
| `c-i32` | `int32_t`, direct signed argument/result or status | Exact Zryna `i32` |
| `c-int` | C `int`, width/alignment explicitly checked as 4/4 | Explicitly declared `i32` bridge; never canonicalized to `c-i32` |
| `bool32` | `uint32_t`, validated `0` or `1` | Boolean shim only; not general unsigned language support |
| `count` | `size_t`, unsigned 64-bit | Buffer length/capacity slot only; checked conversion, no exposed unsigned language value |
| `bytes-in` | `const uint8_t *` | Read-only synchronous byte borrow; paired count, null-zero rule, maximum and encoding required |
| `bytes-out` | `uint8_t *` | Excluded initially; a later writable-buffer contract needs paired capacity, initialized prefix and new source primitives |
| `bytes-owned-out` | `uint8_t **` | Caller-owned out slot creating one foreign-byte release obligation on success |
| `count-out` | `size_t *` | Caller-owned initialized-on-success out slot paired with a buffer |
| `i32-out` | `int32_t *` | Caller-owned initialized-on-success out slot |
| `handle-in` | `struct T *` | One exact incomplete-struct kind; `borrow` or `consume` explicitly specified |
| `handle-out` | `struct T **` | Caller-owned out slot creating one foreign-handle obligation on success |
| `bytes-release` | `uint8_t *` | Consumes only the exact matching foreign-byte owner; never a general naked pointer |
| `unit` | `void` result | Release or explicitly declared infallible operation; never a parameter |

Pointers have alignment 8 as address carriers; pointed-to `i32` storage requires
alignment 4, count and handle-pointer output storage alignment 8, byte storage
alignment 1. Out slots are distinct from one another and from every borrowed
input range; no pointer depth beyond these forms, pointer arithmetic, generic
`void *`, casts, address forging, global foreign storage or naked pointer escape
is admitted by this vocabulary. A trusted C library must independently promise
valid pointees; metadata cannot check arbitrary pointer validity in process.

Decision **D1** selects the closed source-bound sidecar and the exact reserved
`Ffi.rawCall` unsafe marker plus the named safe primitives in the source contract.
Its complete scalar/buffer/handle source fixtures cover every primitive and bind
exact bytes and spans. Ordinary `declare function`, an
ordinary named module import, a type alias named `Pointer`, or a plain function
call must never silently become an FFI binding. No current v2/v3/v4 protocol is
reinterpreted, and none of these forms currently supplies FFI.

Fixed negative source shapes for that decision include declaration merging and
overloads, optional/rest/default parameters, generic foreign functions/types,
async/Promise, methods/property calls, callback/function-pointer parameters,
direct `_Bool`, bare `number`, inferred `long`/`size_t`, unions, bitfields,
flexible arrays, structs/enums by value, retained borrows, thread-affine or
concurrent handles and cross-language unwind. Each must reject at the syntax or
declaration boundary, with no partial verified set. A provider's TypeScript parse
success is not FFI admission. The fixed review vectors classify all these forms;
the source contract links concrete rejected spellings and token flows. Those
records remain future compiler-rejection requirements, not executed parser tests.

## Exact identity and import/export layout

Fixed tuple: ABI `zryna-native-c-interop-v0` / version `0`, convention
`sysv-amd64-c-v0`, carrier model `native-c-interop-v0-carriers`, owner model
`native-c-interop-v0-resources`, target `x86_64-unknown-linux-gnu`. The native
runtime compatibility input is the exact accepted runtime contract/version for
the selected language, with its target artifact identity separately authenticated.
It is not C's allocator identity and cannot be substituted for one.

A library-contract identity consists of exact logical library name, exact
version, target tuple, exact reviewed header SHA-256, operation-policy SHA-256,
and declared handle/allocator/release keys. `fixture-c-v0` is a logical name,
not a complete identity. Filename, SONAME, symbol alone, a version range, a host
path or the presence of an equal C typedef is insufficient. Acquired artifact
size/digest, linkage and runtime dependency are separate #361/#362 driver inputs;
changing them invalidates link authority even if the declaration stays identical.
No artifact hash is invented for the inert candidate header.

Canonical identity encoding is UTF-8 compact JSON with lexicographically
sorted ASCII object keys, retained array order, integer decimal notation and one
terminal LF. Identity fields use ASCII; digests use 64 lowercase hex characters.
Integers are nonnegative, at most `9007199254740991`; strings use JSON escaping
only for quote/backslash and control bytes, without escaped alternative spellings
for ordinary ASCII. Outcome status values are separately signed 32-bit integers.
Duplicate object keys, noncanonical bytes and unknown fields reject before
hashing; JSON parsing that silently retains the last duplicate is insufficient.
Hash `ZRYNA-NATIVE-C-DECLARATION-V0` followed by one NUL and these exact bytes with
SHA-256. The [fixed identity vectors](../../tests/native-c-abi-v0/review-vectors.json)
pin bytes and digest independently; this is a declaration digest, not
#168 package identity, #361 cache identity or evidence of acquired bytes.
The full declaration digest also binds ordered parameters/resources/outcomes,
execution promises, library identity and source binding. The short vector is an
identity-encoding example, not a complete accepted declaration.

| Layout | Required sealed contents and independent verification |
| --- | --- |
| Import record | Source/declaration identity, library-contract identity, exact symbol, convention/target, ordered ABI carriers, resource/outcome policies, driver-input references; explicit unsafe site plus separately verified safe-wrapper site |
| Export record | Source/function identity, derived exact versioned symbol and visibility, convention/target, ordered boundary carriers, body effects/traps, entry validation, output initialization and cleanup obligations; no import-library alias |
| Call MIR | Exact sealed import index/signature, ABI argument locations, low-width Boolean checks, retained borrows/owners, initialized out-slot map and one complete outcome/cleanup plan |
| Entry MIR | Exact sealed total-scalar export index/signature, scalar entry checks before body, total verified body, full-width scalar result and no unwind; no copy, acquisition, status/out or trap channel in initial v0 |
| Native artifact | ELF64 little-endian x86-64, audited definitions/imports/relocations against sealed records; driver validates exact object/library/sysroot/tool identities before link and atomic publication |

Decision **D2** selects this exact identity layout and closed schema. No concrete IR opcode
or successor wire DTO is assigned here; independent verifier semantics must be
accepted before those representations are implemented. Raw forged target,
signature, source binding, resource kind, status, output initialization, cleanup
role or stale digest must fail without constructing a sealed value. MIR cannot
erase an IR release obligation or introduce a new external symbol.

The separate [#361 D2 package alignment](../package/RESOLVED_BUILD_PLAN_V0.md#proposed-d2-native-c-v0-alignment)
is a prospective specification-only successor, not appendix acceptance recorded by this #364 review.
PR #393 accepted only the source-only contract with a draft native appendix. This successor defines
explicit proposed and specification-only states with the same closed #364 tuple, preserves both
historical grammars without auto-upgrade, and returns denied native admission in either state.
Its prospective decision below requires actual independent successor review and normal integration.
That decision can resolve D2's #361 native-appendix-contract prerequisite only; runtime artifacts,
IR/MIR implementation, policy admission, linking, FFI conformance and public support remain separate.

### Prospective D2 native-appendix-contract decision

This is a pending repository decision record, not a maintainer sign-off or executed-conformance
receipt. The parent reports an independent read-only review of the exact predecessor below found
no substantive contract defect. That report does not approve this successor. Unknown reviewer
identity and missing approval/integration outcomes remain pending until genuine results exist.

| Decision input or outcome | Recorded fact or pending requirement |
| --- | --- |
| Decision scope | #361 specification-only native appendix alignment for #417 D2; no execution authority |
| Independently reviewed predecessor | [PR #537](https://github.com/zryna/zryna/pull/537), head `5cb4a14db3cb49ed3df059d76436f3be1cc48890` |
| Predecessor source tree | `d7afa01f528d9d6ecf3bec13cf380823041fe477` |
| Locally checked predecessor before app publication | `2edb601eff7670606058532600d0ce5d127ab884`, identical source tree; not the published revision |
| Accepted #364 specification reference | [PR #504](https://github.com/zryna/zryna/pull/504), reviewed head `bb92f44e1c1de2637163f7deb25a8c157cad6d29`, normal integration `65be51f6e1916ee8677d1cbf536ce2a8caf774a4` |
| Successor revision under review | Pending; exact immutable candidate head/tree must be recorded in its independent review and PR evidence |
| Independent successor reviewer identity and disposition | Pending; do not infer identity or approval from authorship, tests or predecessor review |
| Linux/Windows build-plan contract receipts | Pending genuine exact-revision runs, including both D2 states and negative cases |
| Repository approval decision | Pending real independent outcome; no retroactive PR #393 native approval |
| Normal integration revision | Pending actual integration; no merge or issue closure recorded here |
| Effective specification disposition | Pending approval and normal integration; this candidate remains prospective |

The successor review must check the identical fixed ABI tuple and closed constraints in both new
states, string/numeric wire-type boundaries, unchanged source-only canonical bytes/key/receipt,
legacy/proposed non-upgrade, runtime-input versus authenticated-byte separation, and continued
native-recipe denial. `denied-specified` is specification metadata, never an execution/acquisition,
cache-reuse, link, publication or grant capability. Actual approval and integration may establish
only this D2 contract prerequisite. No language-runtime artifact, concrete IR/MIR representation,
foreign call, process isolation, #362 execution admission or #168 provenance is proved by it.

Scalar imports/exports use signed 32-bit lanes; Boolean entry/result validation
uses low 32 bits and accepts only 0/1. Narrow excess register bits are unspecified.
Pointer/count lanes are 64-bit INTEGER; out records are caller storage, never a
by-value public tuple/struct. Buffer imports use pointer plus count and exact
out slots; opaque imports use distinct library-specific kinds. A buffer or
handle export requires its own operation table with C-client allocation/release,
stale-use, malformed-input and copy-failure policy; the existing scalar reverse
fixture supplies none of that evidence. Decision **D3** excludes buffer/handle
and fallible exports from initial v0; any later admission requires a new review.
Do not treat a broad pointer row as accepted buffer/handle export support.

## Ownership and failure reconciliation

Every operation, including releases and exports, declares `none` when no foreign
allocator/owner transition applies. An output slot is caller-owned and unread
until its successful status initializes it. A successful non-null resource creates
an obligation before metadata conversion; it is never a private M3 owner.
Within one wrapper frame acquire in declared result order, clean in reverse
acquisition order, and remove an obligation only after confirmed matching release.
Already released bytes do not reenter cleanup of an earlier handle. Shared/Weak
count fields, String/Vec headers and private helper symbols never cross the ABI.

On a valid successful byte result, private-copy preparation may trap: release
foreign bytes once, reverse-clean earlier live obligations and retain the exact
controlled trap. On malformed metadata, release only under the reviewed guarantee
that the returned non-null pointer remains a live releasable allocation despite
that defect. Without it, record an unresolved foreign obligation and host/ABI
failure, with no guessed dereference/free or leak-free claim. Wrong allocator,
wrong library, stale token and repeated release reject before C entry. A release
fault leaves the obligation unresolved; never retry or report successful cleanup.

`fixture_open` status 1 means a negative seed with no allocation/output write.
Decision **D4** fixes allocation failure for a nonnegative seed as status 2 with
the same no-allocation/no-write guarantee. A negative seed is checked first,
returns status 1 and never attempts allocation. No alternative pool is inferred.
`fixture_copy_bytes` already declares allocation failure as status 1.

The direct `add` import and reverse export wrap modulo 2^32. The reference uses
unsigned arithmetic and in-range signed reconstruction; no invented overflow
trap or export status is added. Direct `_Bool` is excluded. Boolean tests need a
separate declared Boolean shim/export: passing 2 to `add` tests an ordinary valid
`i32`, not a Boolean rejection. Fallible exports need an explicit status/out
contract with no output on failure and preserved controlled-trap identity.
Decision **D5** excludes such exports initially and fixes the closed typed
`ForeignError` wrapper/driver outcome in the source contract, independent of
internal ownership-runtime statuses or unaccepted Option/Result.

Recoverable declared C status, controlled language trap, host/ABI failure and
process failure remain four different observations. In-process C faults provide
no cleanup guarantee. Manifest restrictions do not sandbox native syscalls;
#357 rejects unverified effects when denied capabilities/isolation are required.

## Exact limits and accounting

These values are decision **D6**, not existing M3 or runtime limits. Every bound
has a fixed exact/first-extra vector and independent future rejection test.
Limits intersect existing source/syntax/M3/layout/driver limits; none expands one.

| Metric / accounting boundary | Fixed maximum | First extra / outcome |
| --- | ---: | --- |
| Declaration-set wire bytes, before decode including LF | 1048576 | 1048577 / declaration resource-limit |
| Libraries across the complete resolved runtime closure | 16 | 17 / declaration resource-limit |
| Operations, imports plus exports, across that closure | 256 | 257 / declaration resource-limit |
| Parameters including raw pointers/count/out slots, per operation | 16 | 17 / declaration resource-limit |
| Resource-policy groups per operation | 8 | 9 / declaration resource-limit |
| Exact symbol, logical/parameter name, library id/version ASCII bytes, separately | 128 | 129 / declaration resource-limit |
| Export logical-name bytes before the fixed 13-byte prefix | 115 | 116 / declaration resource-limit |
| Qualified operation/kind/allocator/release key ASCII bytes | 257 | 258 / declaration resource-limit |
| Portable source path ASCII bytes | 256 | 257 / declaration resource-limit |
| Primitive call-spelling UTF-8 bytes | 4096 | 4097 / declaration resource-limit |
| All string-value UTF-8 bytes, every occurrence including ids/digests/tags/spellings, excluding keys/framing | 65536 | 65537 / declaration resource-limit |
| Authenticated source files | 256 | 257 / declaration resource-limit |
| Primitive source sites | 4096 | 4097 / declaration resource-limit |
| Declared statuses per operation | 16 | 17 / declaration resource-limit |
| Foreign kinds per library | 16 | 17 / declaration resource-limit |
| Allocator pairs per library | 16 | 17 / declaration resource-limit |
| Wrapper byte length/capacity per buffer | 4096 | 4097 / controlled bound trap before C entry |
| Simultaneously live foreign obligations per execution instance across wrapper frames | 64 | 65 / controlled bound trap before acquisition call |
| Declaration diagnostics including terminal budget diagnostic | 256 | 257 / slot 256 replaced with terminal resource diagnostic |

Wire depth is bounded at 16 (root depth 1, scalar leaf does not add depth); depth
17 rejects before unbounded parse/descent. No recursive policy/type grammar is
admitted. Library/operation collections have canonical bytewise identity order;
parameters, resources and statuses preserve declared order. Duplicate references
do not create a second library or allowance; distinct library identities count
separately. Count imports and exports even if unused. Count a live foreign byte
owner until its release succeeds, including conversion and cleanup; borrowed
inputs and uninitialized output slots are not new foreign owners. Preflight every
call's maximum acquisitions with checked arithmetic before its first foreign effect.
Raw calls remain subject to declaration bounds but their documented unsafe
preconditions cannot be advertised as runtime wrapper checks. A design-only limit
oracle is not execution proof or an OS memory/process quota.

Canonical error order is wire/shape/budget, identity, target/profile, ABI/signature,
resource policy, source/wrapper/IR, artifact/link and execution. The source contract
fixes required producing codes C4100-C4108 without reusing #357 C4010-C4015 or M3 meanings.
Errors use authenticated spans or a global/workspace location; never fabricate
source spans for sidecar/link inputs. Rejection is atomic; later valid input must
verify independently without inheriting stale owners or diagnostics.

## Concrete decisions and bounded delivery

| Decision | Fixed contract rule | Implementation prerequisite |
| --- | --- | --- |
| D1 | Closed source-bound sidecar; exact `Ffi.rawCall` and named safe primitives; no alternative grammar | Source/protocol implementation |
| D2 | Exact ABI/library/declaration identity and sealed import/export obligations defined here and in the closed schema | #361 native appendix acceptance; IR/MIR implementation |
| D3 | Only total scalar C exports; buffer/handle and fallible exports excluded from v0 | C export declaration verification |
| D4 | `fixture_open` allocation status 2, no output writes or new obligation | Tiny C fixture and failure injection |
| D5 | Closed typed `ForeignError` wrapper outcome and operation-specific statuses; no fallible C exports | Wrapper/driver outcome lowering |
| D6 | Exact counts, accounting, limits and C4100-C4108 producing diagnostics | Decoder/declaration/wrapper verification |

All six decisions and the Boolean/namespace/allocator/fault rules are fixed in
this complete schema/source/fixture contract. Normal integration records #364
specification acceptance; no earlier proposal is retroactively attributed sign-off.
There is no remaining local design choice or current FFI implementation claim.
Before publication, run applicable light design checks, then repository-required
frozen install, preflight and M0 in a coordinated build slot on the final reviewed
revision. Linux/Windows hosted requirements govern integration. No M3 gate,
digest-pinned inventory, website whitelist or existing selector changes.

After normal specification integration: an isolated tiny prototype first; then
independent declaration and IR/MIR verification; audited import/export/linking and generated header;
wrapper/cleanup/fault conformance; SQLite and Rust-through-C-shim as separate
library proofs; per-platform evidence; explicit public activation last. A prototype
does not satisfy #417's complete conformance. Keep specified, prototype,
conformance-passed and publicly-supported evidence separate at every stage.
