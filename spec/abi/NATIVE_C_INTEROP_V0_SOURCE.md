# Native C v0 sidecar and source forms

State: **specified-only normative future contract**, for #364, effective upon
normal integration. These spellings and records are fixed future requirements.
The current parser, compiler, CLI and runtime accept none
of the FFI forms here. This document selects the sidecar design; it does not
offer a second source grammar alternative or activate a selector.

## Closed sidecar syntax

The [closed JSON Schema](../../schemas/zryna-native-c-declarations-v0.schema.json)
defines `zryna.native-c-declarations.v0`. All fields shown in its `required`
arrays are mandatory; unknown fields, null except explicitly nullable fields,
unknown enum tags and missing fields reject. The complete
[canonical fixture](../../tests/native-c-abi-v0/declarations.ffi.json) includes
seven imports, one total scalar C export, exact library/header/policy identities,
source bindings, every resource policy and every source primitive site.
It is a design fixture, not a currently accepted compiler input.

One authenticated source set supplies the .zry files and one separately
authenticated sidecar supplies declarations. Neither is discovered by ambient
filesystem search. The driver retains both byte identities; future provider
syntax must authenticate exact source call/declaration nodes, file sets and byte
spans independently of the sidecar's claims. A file hash alone supplies no AST
authority. The source primitive namespace is reserved and cannot be shadowed,
imported, assigned, aliased, called indirectly or implemented by user code.

`sources` is bytewise path ordered, `libraries` by exact id, `operations` by key,
and `sites` by `(path, start, end)`. Duplicate identities or sites reject.
Each site binds exact call bytes plus its primitive and explicit safety tag.
Import operation bindings select one authenticated raw/release call; repeated
calls reference that exact declaration. Export bindings select one complete
source function declaration and ordinal. Import keys are `library-id/symbol`;
export keys are `output-module-id/logical-name`. Exact library version is a
separate mandatory field and part of identity; ids are not filesystem paths.
Library ids use `name@version`, with exact ASCII name/version and no range or
path segments; the separate version must equal the id's suffix. Qualified
operation/kind/allocator keys may use 257 ASCII bytes, allowing a 128-byte
library id, one slash and a 128-byte symbol without silently narrowing that cap.
`sourceBinding.ordinal` is the zero-based canonical operation-record index,
not a provider node id or a claimed ordinal inferred from source discovery.

Library kinds and allocators have explicit identities and allocator categories
`handle` or `bytes`; no kind is inferred from its spelling. The policy digest
hashes canonical `{allocators, kinds, operations}` for that library, excluding
operation `sourceBinding` only. The complete declaration digest additionally
binds every source binding and site. Header bytes are authenticated separately.
Every create/release reference resolves within the same library and exact kind;
one kind cannot be redirected to a different allocator or a same-typed release.

`parameters` retain ABI order. Each pointer/count belongs to its exact indexed
resource group; `i32-out` is caller storage with no foreign owner. Read-only
byte groups have ordered `[bytes-in,count]`, owned-byte creation has
`[bytes-owned-out,count-out]`, handle creation `[handle-out]`, handle borrow or
consumption `[handle-in]`, and byte consumption `[bytes-release]`. Resource
slot references must be bidirectional and unique. Status `0` initializes exactly
the declared output slots and creates exactly the declared owner groups
(a canonical empty byte result creates no live allocation). Each nonzero
declared recoverable status writes no output and creates no new obligation.
Unknown statuses are host/ABI failures. Direct and void modes have no status list.
Each status also freezes its condition and unchanged-input guarantee. Creating
resources explicitly promise freshness; a byte copy's `expectedLengthSlot`
binds its returned count to the exact input-count slot, so validation does not
infer length equality from the symbol name. A missing guarantee is never repaired
by a compiler assumption.

Initial v0 C exports are **total scalar functions only**: `i32`, explicit C-int
bridges or canonical Boolean shims, with no owned inputs, allocation, foreign
calls, host effects or controlled traps. Buffer/handle exports, unit/status-out
exports and all fallible C exports are **excluded from v0**. They require a new
reviewed version with exact C-client policies before admission. Borrowed writable
buffer imports are also excluded from this initial sidecar/source vocabulary;
the general ABI table's writable-buffer row is a later-version design obligation,
not permission to pass a read-only loan to C as writable memory.

## Exact source primitives

Calls use this bounded production, within an otherwise separately verified
language body:

```text
ffi-call = "Ffi." primitive "(" arguments ")"
operation-literal = '"' exact-operation-key '"'
arguments = empty | typed-expression ("," typed-expression)*
```

Only the names and argument/result shapes below are admitted. Operation and kind
keys are literal ASCII strings, not computed values, escaped aliases or version
ranges. Ordinary functions, imports or declarations cannot supply these intrinsic
meanings. These calls need a separately versioned foreign syntax authority; do
not reinterpret protocol v2/v3/v4 or depend on native frontend replacement.
`typed-expression` is an exactly typed selected-language scalar/owned expression,
a local foreign token, or an admitted nested intrinsic. Each primitive's table
fixes arity and types; raw calls have one literal key plus exactly the bound C
parameter count. No spread/rest/optional argument, generic call, method receiver,
bracket access, optional chaining or indirect callee is admitted.

| Exact spelling | Source arguments and result | Safety and lifecycle |
| --- | --- | --- |
| `Ffi.rawCall(key, ...arguments)` | Literal import key and exactly ordered ABI arguments; returns declared `i32`, `bool` or unit | Explicit unsafe/raw marker, also tagged `unsafe-raw` in the sidecar; signatures are checked but arbitrary C is trusted only under the reviewed library contract |
| `Ffi.borrowBytes(value)` | Owned `Vec<i32>` whose elements are all 0..255 -> scoped `FfiBytes` | Check length <=4096 and element range before creating initialized private byte storage; retain source, no naked pointer or C retention |
| `Ffi.borrowUtf8(value)` | Owned `String` -> scoped `FfiBytes` | Retain well-formed complete UTF-8 bytes; check byte length <=4096, no terminator implied |
| `Ffi.byteLength(loan)` | `FfiBytes` -> `i32` | Exact byte count; raw lowering checks the explicit `size_t` conversion |
| `Ffi.outI32()` | no arguments -> `FfiI32Out` | Fresh caller-owned aligned output slot, unread before successful initialization |
| `Ffi.outHandle(kind)` | literal exact kind -> `FfiHandleOut` | Fresh non-aliasing output slot with one sealed kind; reserve possible acquisition before call |
| `Ffi.outBytes()` | no arguments -> `FfiBytesOut` | Fresh byte-pointer output slot; no pointer exposed to source |
| `Ffi.outCount()` | no arguments -> `FfiCountOut` | Fresh count output slot; paired with its exact byte-pointer slot |
| `Ffi.readI32(slot)` | initialized `FfiI32Out` -> `i32` | Only after the exact operation's status 0; rejects an uninitialized, aliased or stale slot |
| `Ffi.takeHandle(slot)` | successful `FfiHandleOut` -> `FfiHandle` | Moves the already recorded non-null obligation; retains exact library/kind/allocator/release identity |
| `Ffi.takeBytes(pointerSlot,countSlot)` | successful paired outputs -> `FfiOwnedBytes` | Validate null/length/freshness/library promises before reading; record non-null obligation before validation; canonical empty token has no allocation |
| `Ffi.copyBytes(owner)` | live validated `FfiOwnedBytes` -> private `Vec<i32>` | Complete checked copy of bytes as 0..255; foreign owner remains until release; preparation failure releases it and earlier live obligations in reverse |
| `Ffi.release(key,owner)` | exact release key and live matching foreign token -> unit | Consume once after confirmed infallible release; empty byte token consumes without calling C; wrong/stale/repeated token rejects before C entry |
| `Ffi.foreignError(key,status)` | exact status-mode import key and declared nonzero recoverable `i32` status -> terminal outcome | Reverse-clean only still-live obligations and produce typed `ForeignError`; never read out slots or turn status into a language value/trap |

The seven `Ffi*` token names above are compiler-owned types in this specified
foreign extension only. They are not aliases for private M3 storage, general
pointers or public scalar ABI types. `FfiHandle` and its output retain a sealed
nominal library/kind internally even though the source spelling is common.
Handle/owned-byte tokens move linearly; loan and slot tokens are scoped,
non-escaping and cannot enter fields, containers, closures, returns or the C
export ABI. No cast, clone, equality/address observation, arithmetic or C-retained
borrow is allowed. A token identity is never reused after consumption.

Fresh slots cannot alias loans, one another or owner storage. A wrapper may use
one loan for repeated synchronous calls while retaining its source and private
backing storage; the C borrow ends on each call return. Source lowering and
independent IR/MIR replay must prove status dominance for out reads, nominal kind,
initialization, transfers, maximum acquisition, cleanup and nonescape. Matching
syntax or passing the schema supplies none of that runtime proof.
Initially every foreign token stays in its creating wrapper function: even owned
foreign tokens cannot cross private function parameters/returns. A private caller
may receive only the separately verified scalar/private-owned result or terminal
failure. CFG joins require identical live-token/slot states on all incoming paths;
loops must preserve that state and prove the acquisition bound. Reads are dominated
by the exact call's status 0, and successful takes invalidate their output slots.
Consumption invalidates every alias; no unchecked branch, exceptional exit or
loop backedge can skip the complete cleanup plan. These restrictions do not enlarge
any accepted M3 syntax, layout or verification budget.

The future authority chain is authenticated source bytes plus new foreign syntax
authority and sidecar, independently verified foreign declarations/typed IR,
independently verified native call/entry MIR, then audited object and driver link
authority. Each stage replays the predecessor identity and resource facts; no
producer flag, schema pass or recorded digest substitutes for that replay. Existing
M3 source/IR/MIR authorities remain valid only for their existing vocabulary.
Composing them with this extension requires a new explicitly reviewed authority;
the specified foreign token types, calls and outcome tags cannot be injected into
the old sealed values. Both positive source forms and hostile raw IR/MIR records
must be tested independently before implementation or selector activation.
Concrete native slot storage is zero-initialized before C entry, while the
verifier's logical output remains unreadable until status 0. Native initialization
prevents an uninitialized-memory read if trusted C violates its write promise;
it does not prove a write occurred. The independent C fixture must instrument
output writes, not infer failure atomicity from equal sentinel values alone.

The complete positive source examples are
[scalar/reverse export](../../tests/native-c-abi-v0/source-scalar.zry),
[buffer borrow/copy](../../tests/native-c-abi-v0/source-buffer.zry), and
[handle acquire/read/release](../../tests/native-c-abi-v0/source-handle.zry).
They are inert reference-source fixtures, deliberately outside the M3 corpus.
All primitive sites in them have exact independently checked bytes/spans in the
sidecar. They have not been accepted by any current source provider or executed.
The [concrete negative source records](../../tests/native-c-abi-v0/source-negatives.json)
fix rejected spellings and token flows with their expected future boundary. Current
documentation tests verify this review inventory only; they do not parse those
records or claim that a compiler rejected them.

## Fixed failure carrier and fixture statuses

The required foreign entry observation is the closed tagged carrier
`Returned(typedValue)`, `ForeignError(operationIdentity,statusI32)`,
`ControlledTrap(exactLanguageTrapIdentity)`, `HostAbiFailure(category,unresolved)`
or `ProcessFailure(harnessObservation)`. This is a verifier/driver observation
contract, not a source `Option`/`Result` or new C representation. A terminal
`Ffi.foreignError` cannot construct a scalar result or continue source execution.
Private callers propagate the same operation identity/status after verified
cleanup. Source catching or pattern matching this carrier is excluded initially.

The safe-wrapper entry checks length, range, nullability, status and resource
identity before C entry or value exposure. Over-limit byte lengths, invalid
byte elements and live-obligation reservation exhaustion are controlled traps
`FOREIGN_LENGTH`, `FOREIGN_BYTE_RANGE` and `FOREIGN_RESOURCE_LIMIT` respectively;
these specified foreign-extension identities do not replace existing M3 traps.
Private M3 copy allocation failure preserves its existing exact trap identity.
Unknown status, malformed output, illegal owner/slot use and release defects
are host/ABI failures. A release failure overrides an otherwise recoverable/trap
completion with unresolved cleanup and no retry. Foreign process faults retain
the existing no-guaranteed-cleanup boundary.

| Raw operation | Fixed successful status / result | Fixed recoverable status and no-output/no-new-owner guarantee |
| --- | --- | --- |
| `add` | Direct modulo-2^32 `i32`, total | none |
| `sum_bytes` | 0 initializes only sum output for n<=4096 | 1 for raw n>4096; safe wrapper traps before that call |
| `fixture_open` | 0 initializes one non-null handle | 1 negative seed; 2 allocation failure for nonnegative seed |
| `fixture_read` | 0 initializes only seed output, retaining handle | none; any other status is host/ABI failure |
| `fixture_copy_bytes` | 0 initializes both outputs; NULL,0 for empty; one fresh allocation otherwise | 1 raw over-limit or allocation failure, neither out written |
| `fixture_close`, `fixture_release_bytes` | void, infallible with matching live non-null input | none; fault leaves unresolved obligation |
| `zryna_c_v0_e_add` | Direct modulo-2^32 `i32`, total | none; no status/trap channel |

Open's allocation failure is now fully specified: allocate nothing, write no
out slot, preserve earlier resources; independent injection must later observe
status 2. Negative seed is checked first and deterministically reports status 1
without attempting allocation. No preallocated-pool alternative remains.

## Fixed producing diagnostics and bounds

These codes are specified for the new declaration authority; no existing
diagnostic implementation or meaning is changed:

| Code | Phase and fixed category |
| --- | --- |
| `ZRYNA-C4100` | wire: invalid UTF-8/JSON, duplicate keys or noncanonical bytes |
| `ZRYNA-C4101` | shape: closed schema, missing/unknown fields or tags |
| `ZRYNA-C4102` | identity: ordering/duplicates, header/policy digest, library/symbol/source identity mismatch |
| `ZRYNA-C4103` | target/profile: unsupported selection, including transitive non-native requirement |
| `ZRYNA-C4104` | ABI: excluded carrier/signature/export or missing admitted convention |
| `ZRYNA-C4105` | resource: allocator/kind/slot/status/failure-atomicity contradiction |
| `ZRYNA-C4106` | source: unauthenticated span, wrong intrinsic marker/key, shadowing or escaping token |
| `ZRYNA-C4107` | resource-limit: exact metric, maximum and first-extra observation |
| `ZRYNA-C4108` | terminal declaration report: replaces candidate slot 256 when further diagnostics would exceed the cap |

Selection failure precedes IR/backend dispatch for universal, JavaScript,
WebAssembly, component and `all`; approved native metadata remains no sandbox.
Wire size/depth are checked before decode. Schema limits include 256 sources,
4096 sites, 16 statuses per operation and 16 kinds/16 allocator pairs per library
in addition to the main review table. Count all string-value UTF-8 bytes,
including repeated ids/digests/tags/source spellings, excluding JSON keys/framing,
toward the 65536-byte string ceiling. Outcome integers are declared nonnegative
signed-i32 C statuses; unknown negative statuses are host/ABI failure.
Sidecar collections and wrapper obligations remain independently bounded; no
fixture/schema check proves the 64-live-obligation runtime invariant.

The documentation checker validates only fixed sidecar/schema/identity/source
and policy consistency. Independent malformed records do not come from a matching
compiler producer. Later compiler conformance must exercise every exact/first-extra
axis, full lexical/source and raw IR/MIR verification, recovery and runtime cleanup;
it cannot substitute this checker for the mandatory verifier or M0/M3 proof.
