# Minimal core and host libraries v0

State: **specified-only; no library implementation or public activation**.
This bounded decision for [#358](https://github.com/zryna/zryna/issues/358) belongs to
[M4](../../docs/ROADMAP.md#m4--webassembly-components-and-wasi), under planning index
[#356](https://github.com/zryna/zryna/issues/356). API names below are decision labels and
semantic operation shapes, not installed modules, package names, import syntax or new selectors.
Acceptance of the pure-core decisions is independent of host/resource implementation. Profile
mapping follows the accepted [cross-target composition contract](../language/CROSS_TARGET_PROFILES_V1.md)
from [#357 / PR #374](https://github.com/zryna/zryna/pull/374), merged at
`ca6307b122e20b071728914a6fdc2d1e7418f083`.

## Authorities and availability

The [current public status](../../docs/M3_PUBLIC_PROFILE.md) governs implemented availability.
[ControlFlowV1](../language/CONTROL_FLOW_MODULES_V1.md) owns scalar operations and direct calls;
[DataOwnershipV1](../language/DATA_OWNERSHIP_V1.md) owns types, moves, borrows and cleanup.
[Scalar ABI v1](../abi/SCALAR_V1.md) admits only exact `i32`/`bool` entry parameters/results.
[Ownership runtime v1](../abi/OWNERSHIP_RUNTIME_V1.md) is compiler-private, not a library or
foreign ABI. Internal exports from dependency modules do not become public entry exports.
Historical implementation checkpoints do not override the integrated public profile.

No runtime, compiler, resolver, package publication, public selector or M3 acceptance item is
added here. Existing M3 #89 -> #90 closure and digest-pinned inventories remain unchanged.
The proposed M4 scope extension is limited to this contract and its later bounded delivery
decisions; it does not require all M7 work before a limited core library.

| State | Evidence needed for this library surface | Current claim |
| --- | --- | --- |
| specified | reviewed API decisions, exact prerequisites and accepted affected dependency sections | profile mapping aligned with accepted #357; resource implementations remain separately gated |
| prototype | separately authorized private implementation using only admitted operations | absent |
| conformance-passed | independent fixed-result, negative, cleanup, boundary and target/host evidence | absent; document checks are not execution |
| publicly-supported | explicit activation, compatibility decision and authenticated support documentation | absent |

## Language prerequisite matrix

References L1-L3 identify available language operations, not an existing library. F1-F6 are
separately gated future prerequisites; none is approved or implemented by this document.

| Gate | Exact authority or missing contract | Admission boundary |
| --- | --- | --- |
| L1 | ControlFlowV1 sections 2-6: exact `i32`/`bool`, wrapping addition, signed comparison, direct acyclic calls, `if`/`else` and return | existing `control-flow-v1`; DataOwnershipV1 must verify the same source separately; no division, remainder, bitwise operators or implicit conversion |
| L2 | DataOwnershipV1 sections 2, 5-8, 11: owned UTF-8 literals, explicit String clone/concat, move, preparation before replacement, reverse cleanup | existing `data-ownership-v1` admitted source; String stays internal; no byte indexing, slicing, locale rules or external decode API |
| L3 | DataOwnershipV1 sections 5-9, 11: exact compiler-known `Vec<i32>`, constructor, structural clone, Copy-element indexing and push | existing `data-ownership-v1` admitted source; exclusive owner for mutation; no user-defined generics, pop, iterators or borrowed imports |
| F1 | #359 JS/WASM conversions and resource adapters; #364 native foreign-resource contract, only for the native rows | accepted scalar/String/list/resource representations, encoding, allocator pairing, borrow interval, partial-failure cleanup, invalid-handle policy and independent interface verification |
| F2 | separate bounded concrete host-outcome and foreign-resource language/ABI decision | operation-specific closed outcomes, owned handle acquisition/use/release and infallible disposal; M3 enums alone do not admit foreign calls, public enums or user destructors |
| F3 | separate exact unsigned 64-bit value, arithmetic and host-carrier specification | needed only for H4 timestamp; current `i32`/`bool` cannot carry a WASI timestamp by coercion; no invented pair-of-lanes ABI |
| F4 | separate async/task specification and corresponding #359/#364 adapter gates | suspension, task ownership, cancellation, polling, completion, cleanup, scheduler and callback/reentrancy rules; no Promise, async, thread or escaping borrow support today |
| F5 | separate bounded JSON/text decoding specification and admitted implementation operations | byte/character access, UTF-8/escape validation, number grammar/range, depth/byte/node limits, concrete parse outcomes and duplicate-key policy; String/Vec existence is insufficient |
| F6 | separate M7 general-generics/monomorphization and `Option`/`Result` specifications, implementation and conformance gates | generic collections and generic error APIs deferred; compiler-known `Vec<i32>` does not enable arbitrary generic declarations |

## Profile matrix and composition

P0-P5 are local table references, never selectors. The accepted #357 row identities are
U-JS, U-WASM, U-NATIVE, JS-BROWSER, JS-NODE, WIT-BROWSER, WIT-COMMAND, WIT-SERVER and NATIVE-HOST.
A backend target, selected language profile and host
permission set are separate axes. A compiler running under Node gains no target environment
permission. `all` requests existing backends, not a WASI world. No profile version implies
language subtyping or permits relabeling precompiled authority.

| Profile set | Accepted #357 rows | Exact ceiling and availability |
| --- | --- | --- |
| P0 | U-JS, U-WASM, U-NATIVE | existing pure-source execution with the row's L gate; empty host requirements; JavaScript/core WebAssembly on supported Linux/Windows hosts; native run only on supported Linux x86-64 |
| P1 | JS-NODE, WIT-COMMAND, NATIVE-HOST | environment eligibility only; F1/F2 and explicit operation/grant enforcement pending |
| P2 | JS-NODE, WIT-COMMAND, NATIVE-HOST | filesystem eligibility only; F1/F2 and explicit preopen/descriptor enforcement pending |
| P3 | JS-BROWSER, JS-NODE, WIT-SERVER, NATIVE-HOST | outgoing HTTP eligibility only; F1/F2 pending; WIT-COMMAND socket imports do not establish an HTTP adapter |
| P4 | JS-BROWSER, JS-NODE, WIT-COMMAND, WIT-SERVER, NATIVE-HOST | monotonic clock eligibility only; F1/F2/F3 pending |
| P5 | JS-BROWSER, JS-NODE, WIT-COMMAND, WIT-SERVER, NATIVE-HOST | secure randomness eligibility only; F1/F2 pending |

Pure code may later be included in compatible host deployments only after separate language and
interface checks; P0 does not certify browser, Node library, Component or foreign embedding.
Allocation under L2/L3 is not ambient I/O and grants no host allocator access.
All H rows are unavailable today, including when selected alongside `data-ownership-v1`.
WIT-BROWSER permits none of the five host candidates.

[#167's WIT contract](../wit/CAPABILITY_PROFILES_V1.md) remains the sole authority for
`zryna:capability-profiles@0.1.0`, WASI `0.2.12`, exact interfaces, world audits, quotas and
granted/denied fixtures. H1 maps to command environment; H2 to command filesystem; H3 to server
outgoing HTTP; H4 to command/server monotonic clock; H5 to command/server secure randomness.
Eligibility never grants an operation. All unlisted host/profile pairs reject. Native metadata
restrictions are not execution isolation for arbitrary foreign code; #364 must identify the
enforcement and external isolation boundary without inferring a sandbox from a manifest.

Apply #357 to the complete reachable runtime dependency closure, including unused calls.
An explicit empty requirement set must be verified; missing metadata is unknown, not pure.
Recompute the transitive union of exact capability/interface requirements and check each
dependency's restrictions. The root must approve that complete request; effective authority is
its intersection with the profile ceiling and current explicit host grant. All selected targets
must pass before any backend dispatch. Shared-instance quotas include all dependencies and do not
multiply with import count. Build-tool permissions never become target permissions. Diagnostics
retain #357's phase order and canonical root-to-leaf witness; no library-specific composition
algorithm, manifest field or diagnostic code is introduced here.

## Bounded API matrix

Each operation is synchronous at this semantic boundary. `retain` below means observing an
existing owned place through an admitted built-in, not introducing a borrowed import signature.
Host shapes are semantic pseudocode pending F1/F2, not parseable foreign declarations.
No API accepts a caller allocator or exposes capacity, addresses, internal status words or layout.

| API | Layer | Operation and exact v0 behavior | Language gates | Profiles | Ownership | Errors | Oracle |
| --- | --- | --- | --- | --- | --- | --- | --- |
| C1 | pure | add_i32(a: i32, b: i32) -> i32; addition modulo 2^32, interpreted signed | L1 | P0 | O1 | E0 | Q1 |
| C2 | pure | min_i32(a: i32, b: i32) -> i32; signed minimum, equality returns a | L1 | P0 | O1 | E0 | Q2 |
| C3 | pure | select_i32(flag: bool, yes: i32, no: i32) -> i32; choose yes iff flag is true; all call arguments evaluate first, left to right | L1 | P0 | O1 | E0 | Q3 |
| A1 | allocation | text_clone: clone(existing String place) -> independent String | L2 | P0 | O2 | E1 | Q4 |
| A2 | allocation | text_concat: concat(two existing String places) -> new String; exact left bytes followed by right bytes, no normalization | L2 | P0 | O2 | E1 | Q5 |
| A3 | allocation | vec_i32_clone: clone(existing `Vec<i32>` place) -> independent `Vec<i32>` | L3 | P0 | O2 | E1 | Q6 |
| A4 | allocation | vec_i32_read: items[index: i32] -> i32; Copy read, exact checked bounds; operation itself allocates nothing | L3 | P0 | O3 | E2 | Q7 |
| A5 | allocation | vec_i32_push: push(mutable `Vec<i32>` place, value: i32) statement; append once after value evaluation, no unit-return API | L3 | P0 | O3 | E1 | Q8 |
| H1 | host | environment_lookup: authorized key (1-64 UTF-8 bytes) -> found String (0-1024 bytes) or missing; no enumeration or ambient process access | L2, F1, F2 | P1 | O4 | E3 | Q9 |
| H2 | host | file_read_text: one authorized relative path (1-1024 UTF-8 bytes) under a supplied preopen -> complete UTF-8 String, at most 4096 bytes; read-only, no writes or leaked descriptor | L2, F1, F2 | P2 | O4 | E3 | Q10 |
| H3 | host | http_get_text: one authorized endpoint/path (1-1024 UTF-8 bytes total) -> HTTP status i32 in 100..599 plus UTF-8 body of at most 4096 bytes; GET only, no redirects, request body, cookies or streaming | L2, F1, F2 | P3 | O4 | E3 | Q11 |
| H4 | host | monotonic_now: explicit clock authority -> one unsigned 64-bit timestamp in nanoseconds; no epoch/wall-clock, timer, sleep or scheduling API | F1, F2, F3 | P4 | O4 | E3 | Q12 |
| H5 | host | random_bytes: explicit secure entropy authority and count i32 in 0..256 -> owned `Vec<i32>` of exactly count values in 0..255; no seed or insecure fallback | L3, F1, F2 | P5 | O4 | E3 | Q13 |

The API bound is exactly these thirteen rows. A rows intentionally cover concrete existing
operations instead of a generic container facade. Source packaging and named nominal type
imports need their own accepted package/language decisions before any reusable installed API.
H rows are conditional v0 candidates, not implementation-ready APIs. Their per-call byte bounds
are proposed library restrictions below the host ceiling, never changes to #167 quotas. Required
authority and quotas apply even to an empty output. H5's i32 byte representation avoids inventing
a source `u8`; its external list conversion still needs F1 validation. H1 missing is a concrete
outcome, not `null` or `Option<String>`. H3 HTTP 404 is a successful transport result carrying
status 404, not a language trap. No M7 generic error container is smuggled in through these shapes.

If an adapter cannot complete H2/H3 synchronously (including browser fetch), that deployment is
additionally blocked by F4 and a reviewed API revision; there is no hidden event loop, implicit
blocking bridge or promise conversion. P3 describes a capability ceiling, not synchronous browser
availability. Timers, subscription APIs, async callbacks and tasks are excluded from v0.

## Ownership and error matrices

| Rule | Allocation/free owner and lifetime | Failure obligation |
| --- | --- | --- |
| O1 | Copy scalars only; no allocation, borrow, resource or drop obligation | reject bad source/carriers before execution; wrapping addition never becomes an overflow exception |
| O2 | compiler-owned runtime allocates the result; input places retain their owners; explicit result transfer moves its one drop obligation; compiler-inserted lexical cleanup frees it exactly once | prepare before commit; failed clone/concat yields no result, retains inputs until trap cleanup and releases initialized prefixes in reverse order |
| O3 | caller owns the Vec; read copies an element; push needs exclusive access, with compiler runtime growth/release; old storage is released only after successful replacement | negative/first-extra index traps; failed growth retains vector and prepared argument until controlled cleanup, with no append or partially moved state |
| O4 | provisional F1/F2 obligation: caller retains authority/input owners for the call; adapter owns acquired host resources and conversion temporaries until complete result transfer; returned owned data belongs to the caller under L2/L3 cleanup; release through the same allocating domain exactly once | close descriptors/response resources and free initialized conversion prefixes on success or failure; no result or foreign handle escapes on partial failure; ending a borrow never frees its owner |

O4 specifies a required ownership postcondition, not an allocator symbol, layout or handle encoding.
#359 owns JS/WASM allocation/free pairing, borrow duration, stale/repeated release, callback
lifetime, reentrancy and thread rules. #364 owns those decisions for native foreign resources.
Do not use JavaScript garbage collection as proof of release, free foreign memory with the Zryna
allocator, serialize M3 storage, or infer resource cleanup from M3's lack of user destructors.
O4's cleanup postconditions describe normal and recoverable operation paths. Fatal canonical
failures and native cleanup failures follow their distinct adapter rules below; they never become
successful cleanup or an ordinary absence outcome. There is no promise of cleanup after external
process termination. Exact external encodings and release-failure behavior must be accepted before O4
becomes implementable.

| Error set | Category and observable boundary | Cleanup/result rule |
| --- | --- | --- |
| E0 | source/type/profile rejection or scalar ABI validation failure; no language arithmetic trap | no execution for rejected source/carriers; no fallback coercion |
| E1 | existing non-catchable zryna.trap.allocation-v1 or zryna.trap.capacity-v1 | original trap retained after exact reverse live-owner cleanup; no partial result |
| E2 | existing non-catchable zryna.trap.bounds-v1 | no read or later computation after failed index; exact controlled cleanup |
| E3 | provisional closed operation outcomes: invalid-input, not-found where applicable, permission-denied, quota-exceeded, invalid-encoding, io-failure; plus separately tagged language traps, interface violation and host/process failure | F2 must admit concrete operation-specific payloads before use; no generic Result, exception unwinding, silent empty value or host error text as a language result |

E0 source/profile/carrier rejection applies to every row in addition to its listed operation
outcomes. No library call bypasses language verification or the selected interface authority.

E3 ordinary failures return no success payload; H1 missing is its explicit normal absence outcome.
Invalid key/path/count is invalid-input. Oversized host bytes or a host quota refusal is
quota-exceeded without truncation; malformed external UTF-8 is invalid-encoding before String
construction only when received as admitted raw bytes at a recoverable library decode boundary.
A malformed value at an interface declared as text instead follows that adapter's invariant
failure rule; it is not relabeled invalid-encoding. If validated data later exceeds a language allocation/capacity bound, E1 remains
a non-catchable trap. Existing `zryna.trap.utf8-v1` at a language String construction boundary and
`zryna.trap.refcount-v1` remain unchanged, although v0 exposes neither raw-byte construction nor
Shared/Weak APIs. Unknown statuses, forged handles and unconfirmed cleanup are interface
violations, never a recoverable absence. Signals, host exceptions, raw WebAssembly traps and
process exit statuses are host/process failures, not substitutes for typed language outcomes.

Missing/forbidden capabilities reject at #357's owning validation phase before any operation.
For a future `CommandH1V1` H1 lookup on pinned `wasi:cli/environment@0.2.12`, revocation
before the mediated read traps at the host boundary without the effect. Its E3
`permission-denied` outcome belongs to the host execution record: there is no source
`Found`/`Missing` result and no `wasi:cli/run@0.2.12` return, including WIT `err`. The
driver records the denied interface/operation and invalidates the instance. This is the
H1-only transport of #357's revocation rule through an interface with no typed denial
result; it does not admit a general source Result, change the pinned WIT identity, or set
an outcome rule for H2-H5 or other hosts. Other E3 operations still need their own
concrete F2 transport decision before implementation. The H1 one-key, key-byte, and
value-byte ceilings are validated against the captured input before instantiation;
repeated reads of that same value do not deplete a runtime H1 quota. The host's sealed
denial state, set before its deliberate trap, identifies the exact denial reason; an
unrelated raw WebAssembly trap remains a host/process failure, not an E3 host denial.

The future implementation must allocate stable diagnostic codes at the compiler authority for
new rejection categories; these prose labels allocate no codes and do not repurpose #167's codes.

## Affected adapter and native boundaries

The following section-level reconciliation was reviewed against #359 candidate
`e639e3ea82866a53cbe22a958aa9d5d7ff823d17` (`JS_WASM_ADAPTERS_V1.md`, conversion,
allocation and lifecycle sections) and #364 candidate
`d07c070e595bc8ccf738a43627bbd7031cb98bff` (`NATIVE_C_INTEROP_V0.md`, sections 4-7).
These are candidate review references, not merged authorities or executable support. This
document consumes their boundaries without adding operations to either contract.

| Affected APIs | Reconciled constraint | Remaining implementation prerequisite |
| --- | --- | --- |
| C1-C3 | exact scalar carriers and selected language profile; no owned conversion required | verified adapter exports only for later embedding; current source proof remains independent |
| A1-A5 | source-internal String/Vec storage and built-ins remain private; #359's independent string/list-i32 copies are separate public conversions | F1 before any public owned entrypoint; no borrowed import or layout exposure |
| H1-H5 / JS and WIT | #359 v1 admits no effectful JS operation; WIT uses only #167's exact imported interfaces, with no new application exports inserted into pinned worlds | accepted exact host operations and F2 outcomes; source enums/records/Option are outside #359 v1, so neither H1 absence nor H3 status/body is an admitted conversion today; H4 also F3; async realization also F4 |
| H1-H3/H5 / copy and release | #359 distinguishes JS, adapter temporary, language and canonical transfer storage; copy through each owning domain; commit output only after full validation and required normal cleanup | F1 must bind realloc/deallocation/post-return to the exact instance; no public free symbol, foreign allocator substitution or retained memory view |
| H1-H5 / canonical failure | canonical realloc/lifting/destructor/post-return traps are fatal-boundary: invalidate wrappers, discard instance and reclaim independently tracked host resources; no guest cleanup retry or rollback promise after canonical ownership transfer | embedding teardown and independent resource reclamation evidence; do not assume post-return runs after failed lifting |
| H1-H5 / native | #364 pairs each buffer with the exact originating library release, uses failure-atomic declared positive statuses and zero-shaped failed outputs, and confines borrows to the call; returned-text invariant failures trap after committed cleanup | F1/F2 native wrappers and reviewed concrete outcomes; carrier c_u64 alone does not satisfy F3 language support |
| H1-H5 / release failure | #359 ESM repeated dispose is an idempotent no-op, while a second canonical owner drop is invalid; native repeated consume rejects before C; native cleanup failure retains the original outcome and records a distinct host failure without retry | preserve each boundary's policy; no uniform idempotent library destructor or cleanup-success claim |

All host candidates consume external inputs. A failure after a file read, HTTP request, clock
sample or entropy consumption does not undo that effect; v0 performs no automatic retry and
does not return partially converted data. Acquired resources still follow the applicable cleanup
or fatal teardown rule. Callback registration, concurrent entry, reentrancy and async retention
remain excluded by both candidates; library wrappers cannot enable them implicitly. Their future
acceptance or revision affects only the corresponding H rows, not the C/A source-only decisions.

## Fixed oracle and example plan

All Q/N cases are **planned conformance fixtures, not executed results**. Expected values are
fixed independently; agreement among three backends alone is insufficient. Target-internal
allocation/cleanup evidence uses existing private test facilities, never a new public trace API.

| Oracle | Fixed input and exact required observation |
| --- | --- |
| Q1 | add_i32(20, 22) = i32:42; add_i32(2147483647, 1) = i32:-2147483648; add_i32(-2147483648, -1) = i32:2147483647 |
| Q2 | min_i32(7, -3) = i32:-3; min_i32(-2147483648, 2147483647) = i32:-2147483648; min_i32(5, 5) = i32:5 |
| Q3 | select_i32(true, 17, -9) = i32:17; select_i32(false, 17, -9) = i32:-9 |
| Q4 | clone literal "hé"; scalar continuation i32:17; independent private observation bytes [104,195,169] for both owners, distinct result ownership, each released once |
| Q5 | concat "hé" and "!"; scalar continuation i32:17; independent private observation bytes [104,195,169,33]; source owners unchanged and result released once |
| Q6 | clone `Vec<i32>`([7,9]); push 13 only into original; copied[1] = i32:9; copied[2] traps zryna.trap.bounds-v1; original[2] = i32:13 in separate invocation |
| Q7 | `Vec<i32>`([7,9]) index 1 = i32:9; indices -1 and 2 each trap zryna.trap.bounds-v1; empty vector index 0 also traps |
| Q8 | push 13 into `Vec<i32>`([7,9]); index 2 = i32:13; injected growth failure traps allocation-v1, old [7,9] retained until cleanup, no third element initialized |
| Q9 | explicit fixture environment key MODE = "test" returns found "test"; absent key OTHER returns missing; 1024-byte value accepted, 1025 bytes quota-exceeded, no truncated String |
| Q10 | authorized fixture file containing bytes [104,195,169] returns "hé"; 4096-byte valid text accepted, 4097 bytes quota-exceeded; raw file bytes [195,40] invalid-encoding at library decode; all acquired descriptors released |
| Q11 | stub authorized HTTP response status 200 and bytes [111,107] returns status 200/body "ok"; 404/body "missing" remains a response; 4097-byte body quota-exceeded and response released; redirect 302 is returned without following Location |
| Q12 | explicit clock stub returns 4294967297 nanoseconds exactly; no i32 truncation; omitted grant prevents clock invocation (call count 0); real clock values have no fixed wall-time promise |
| Q13 | entropy stub for count 3 returns `Vec<i32>`([0,127,255]); counts 0 and 256 return exactly those lengths; -1 and 257 invalid-input, entropy call count 0; real entropy bytes have no deterministic-value promise |

For A1-A3/A5 add deterministic first-allocation and initialized-prefix fault cases: the original
trap survives, only completed result members release in reverse order, old owners remain until
their cleanup, and no later evaluation occurs. Use bounded injection for allocation exhaustion,
not host exhaustion. Preserve every applicable language/runtime exact-limit and first-extra test.
For H rows add invalid conversion, allocator mismatch, stale resource, repeated release, missing
grant and revocation fixtures at the adapter authority; require no live temporary resource after
each recoverable outcome. For H1-H3 also test maximum input/output bounds, first-extra bytes and
multibyte UTF-8 byte accounting; policy fixtures alone are not runtime cleanup evidence.

The pure-core example plan uses a relative-import chain `main.zry -> choose.zry -> arithmetic.zry`.
`arithmetic.zry` holds the C1/C2 source equivalents, `choose.zry` uses C3, and the scalar entry
composes them. Its complete semantic recipe is:

```text
add_i32(a, b): return a + b
min_i32(a, b): if a < b return a; otherwise return b
select_i32(flag, yes, no): if flag return yes; otherwise return no
score(): return select_i32(true, add_i32(20, 22), min_i32(7, -3))
```

Later source fixtures use existing explicit annotations, canonical `if`/`else` and named relative
imports, first under `control-flow-v1`. Each scalar operation gets its Q1-Q3 cases; `score()`
returns `i32:42`. A separate `data-ownership-v1` compilation checks that exact source without
relabeling M2 IR. These are future fixture files, not a library/compiler prototype in this change.

| Target / host | Required future pure-core proof |
| --- | --- |
| javascript / pinned Node on Linux x86-64 and Windows x64 | all Q1-Q3 and score fixed typed results; zero host imports/calls; canonical scalar carriers |
| webassembly / pinned Node on Linux x86-64 and Windows x64 | same results; independent binary import audit; no WASI or Component reinterpretation |
| native / supported Linux x86-64 link/run | same results through typed observation, not process exit code; audited object/runtime imports |
| native run / Windows x64 | existing ZRYNA-N4002 rejection and no bundle; never count as native execution success |

## Negative and unsupported-feature plan

These are source or abstract future composition fixtures, explicitly distinguished below.
Diagnostic phase and exact stable code must be frozen by the implementing authority before its
conformance gate; an earlier unsupported-syntax rejection cannot prove a later capability check.

| Case | Input / negative example | Required rejection and observable oracle |
| --- | --- | --- |
| N1 | current pure source uses console.log, process.env, Date.now, Math.random, fetch or imports node:fs | current source/module boundary rejects unsupported form before target emission; no ambient call, no bundle; no host exception fallback |
| N2 | future pure main -> library -> host leaf requires filesystem; parent declares empty requirements | #357 forbidden-capability before IR with root-to-leaf witness; zero file calls even if the host grants filesystem or the call is unused |
| N3 | future source claims empty requirements but performs a host clock operation | #357 undeclared-capability before sealed IR at the operation span; zero clock calls |
| N4 | future WIT-BROWSER requests H1-H5, or WIT-SERVER requests H1/H2 | denied by #167 world and #357 composition before instantiation; no partially granted world |
| N5 | future command socket capability is offered for H3 outgoing HTTP | unsupported interface before IR; no inferred HTTP adapter from a network label |
| N6 | accepted future H1 request but no environment grant, then revoked grant | missing-host-grant before execution, then permission-denied at mediated operation in separate fixture; no environment read |
| N7 | current entry exports String, `Vec<i32>`, an outcome enum or a borrowed reference | current public ABI rejects before publication; internal storage is never exported |
| N8 | current source declares generic `map<T>`, uses Option/Result, Vec.pop or JSON.parse | unsupported source/type/operation at its owning phase; no inferred F5/F6 support |
| N9 | current source uses async/await, Promise, task spawn, callback retention or user destructor | unsupported source/ownership form before emission; no scheduler, escaping borrow or cleanup callback |
| N10 | clone source used after move, conflicting Vec mutation while borrowed, or borrowed import | existing ownership/import rejection before publication; retain stable source spans; no implicit clone to repair it |
| N11 | future native foreign-resource library requested for javascript or webassembly | #357/#364 unsupported-target/interface before any selected backend; no partial all-target result or false native sandbox claim |

## Deferred options and bounded implementation breakdown

General maps/sets, iterator/closure libraries, sorting callbacks, `Option`/`Result` utilities and
generic serialization stay behind F6 and their concrete algorithm contracts. JSON remains outside
v0 behind F5: no parser, serializer, reflection model or JSON number conversion is implied by an
owned String. A bounded concrete JSON reader could be specified without waiting for all of F6,
but it still needs F5 and independent malformed-input/limit evidence. Unicode iteration, text
formatting, regex, date/time calendars, broad networking, streams, process launch, standard I/O,
DOM access, database wrappers and package/registry publication are deferred options. Async/task
work needs F4; it is neither a prerequisite for C/A rows nor automatically authorized under M4.

| Later slice | Exact prerequisites before implementation | Measurable conformance / activation gate |
| --- | --- | --- |
| S1 concrete scalar core source | accepted C1-C3 decisions, L1; existing relative modules; #357 only when composing declared profile requirements | Q1-Q3 plus score, N1, fixed three-target results and unsupported-host rejection; reviewed source distribution and support documentation before library activation |
| S2 concrete allocation operations | accepted A1-A5, L2/L3 admitted shapes; source packaging/type imports only if needed by delivery | Q4-Q8, N7/N10, independent cleanup/fault and exact/first-extra evidence on each claimed target; no public owned ABI |
| S3 one host operation at a time, beginning with environment lookup | accepted #167/#357 mapping, corresponding F1/F2 decisions and actual host enforcement; #364 only for native delivery | Q9 and N2-N7/N11 as applicable, grant/revoke and temporary-resource cleanup; accepted host limits, adapter version and explicit activation |
| S4 other bounded host candidates | H2/H3/H5 each needs its own F1/F2 acceptance; H4 also F3; any asynchronous realization also F4 and an API revision | corresponding Q10-Q13, no ambient fallback, malformed conversions and all resource-boundary cases; per-host evidence, never a blanket portability claim |

No slice opens a new issue or changes current acceptance gates here. Package formats/resolution
belong to #168 and M5 successors; installed distribution waits only for the package decisions it
actually consumes. General native FFI and native frontend replacement do not block pure core.

## Dependency alignment checklist

- [x] #357: P0-P5 mapping aligned with accepted PR #374 at the revision recorded above, including
  exact language checks, transitive requirements, all-target rejection, grant phases and native limits.
- [x] #359/#364: reconcile affected API/section boundaries with the candidate revisions recorded
  above; retain incompatible or unavailable conversions as explicit prerequisites, not v0 support.
- [ ] Before host implementation, accept the corresponding F1/F2 operation/conversion and cleanup
  decisions; retain native isolation limits and #167's immutable worlds. H3 command support needs
  a separate interface decision. This is a later implementation gate, not a pure-core review blocker.
- [ ] F2/F3/F4: identify separately reviewed language/ABI decisions before the corresponding H
  row can become implementation-ready; do not turn these into generic M3 or whole-M7 blockers.
- [ ] Run focused documentation/contract checks and repository-required verification on the
  final aligned revision; retain Linux/Windows M0 and relevant cross-target gates before merge.
- [ ] Obtain review of the bounded M4 scope extension and accepted dependency deltas before
  publication; track implemented, conformance-passed and publicly-supported evidence separately.

The [document contract tests](../../tests/minimal-library-contract.test.mjs) check matrix closure,
gated host mappings against #167, fixed scalar oracles and unsupported boundaries. They do not
compile examples, execute a host, prove allocator cleanup or certify profile-composition support.
The document is intentionally outside the website export whitelist until separately reviewed.
