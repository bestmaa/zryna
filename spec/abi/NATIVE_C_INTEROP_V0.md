# Native C interop and foreign resources v0

Contract candidate: `zryna-native-c-interop-v0`. State: **specified-only draft for
[#364](https://github.com/zryna/zryna/issues/364)**. Review and acceptance of these
decisions precede implementation. No foreign declaration, import, export, wrapper,
library, target selector, or public FFI capability is implemented by this document.

## Authority and scope

This is a native-only boundary for `x86_64-unknown-linux-gnu` using the LP64 System V
AMD64 C ABI. It does not change [scalar ABI v1](SCALAR_V1.md), the compiler-private
[ownership runtime ABI v1](OWNERSHIP_RUNTIME_V1.md), M3 selection, or existing M3 gates.
The native FFI profile is a separately verified extension of the
[cross-target composition contract](../language/CROSS_TARGET_PROFILES_V1.md): a native
foreign requirement must fail for universal, JavaScript, WebAssembly, and `all`
selections before backend emission. A native profile declaration is metadata, not
isolation of in-process C code. The driver owns target toolchain validation, linking,
and publication. The optional [native build-plan appendix](../package/RESOLVED_BUILD_PLAN_V0.md)
may carry accepted ABI identities and artifact inputs; it does not define this ABI or
block review of its core decisions.

Foreign declarations enter through provider-neutral syntax and semantics. An independent
Universal IR verifier seals the target, exact external symbol, signature and resource
policy; an independent native MIR verifier seals each call/export and its cleanup edges.
The backend consumes sealed views, emits only audited symbols and relocations, and does
not infer a C type from a Zryna name. Source declarations alone grant neither linker
inputs nor host authority. Link failure never triggers a fallback symbol or library.

## Target ABI table

The [AMD64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI/-/jobs/artifacts/master/raw/x86-64-ABI/abi.pdf?job=build)
([source repository](https://gitlab.com/x86-psABIs/x86-64-ABI)) is the target authority for
LP64 sizes, alignments, argument classification and calling sequence. These are C
object widths and alignments, **not** permission to use every row in source. A C header
compiled for the selected target must confirm `sizeof`, `_Alignof`, signedness where
relevant, and function types with compile-time assertions before a fixture is trusted.
The x32 ILP32 ABI, Windows x64, macOS, other Linux architectures, non-default ABI
attributes, and `-fshort-*`/packing options are different targets or unsupported modes.

| C spelling on this target | Bytes | Alignment (bytes) | v0 boundary decision |
| --- | ---: | ---: | --- |
| `int8_t`, `uint8_t`, `char` | 1 | 1 | Explicit byte lanes use `uint8_t`; plain `char` is not an implicit text or signed-byte mapping. |
| `int16_t`, `uint16_t` | 2 | 2 | Reserved; no direct Zryna source type admitted. |
| `int32_t`, `uint32_t`, `int`, `unsigned int` | 4 | 4 | `int32_t` and C `int` may map only through an explicit checked `i32` declaration; unsigned forms require a separately implemented unsigned language type or a reviewed range-checking wrapper. |
| `int64_t`, `uint64_t`, `long`, `unsigned long`, `long long`, `unsigned long long` | 8 | 8 | Reserved pending exact 64-bit language/IR/ABI support. C `long` is never inferred from `i32`. |
| `size_t` | 8 | 8 | Unsigned 64-bit LP64 count; admissible only as an explicit ABI carrier after checked conversions to and from a supported language length. Never inferred from Zryna spelling. |
| `_Bool` / C `bool` | 1 | 1 | Direct foreign `_Bool` signatures are excluded in v0. C shims use `uint32_t` with exact `0`/`1` validation to match the existing public native Boolean carrier. |
| `void *`, pointer to object, pointer to opaque incomplete struct | 8 | 8 | Address carrier only; pointee layout and ownership come from the declaration, never from pointer width. Function pointers are excluded. |

Only fixed-arity C functions returning `void`, an explicitly admitted scalar, or an
explicit status scalar with caller-owned out parameters are proposed. The initial
implementation slice may use `i32`, canonical `uint32_t` Boolean shims, `uint8_t`
buffers, and opaque object pointers; every other table row remains a reserved exact
mapping until its language and verifier gate exists. No automatic signed/unsigned
conversion, truncation, enum conversion, or Zryna `bool` to `_Bool` bitcast is valid.
Raw C `int`, `long`, `size_t`, and `_Bool` must be named as distinct ABI types in
verified declarations; source spelling cannot silently choose one.

All calls use the System V AMD64 C convention: INTEGER-class scalar/pointer arguments
occupy `%rdi`, `%rsi`, `%rdx`, `%rcx`, `%r8`, `%r9` in order, then the ABI stack area;
scalar integer/pointer results use `%rax` with the declared width. The caller obeys the
ABI's 16-byte stack alignment at call and preserves live caller-saved values as
needed; the callee preserves the ABI's callee-saved registers. Narrow argument/result
high bits are not a validation channel: wrappers check the declared low-width value
and canonical Boolean range explicitly. No C
variadic convention, hidden aggregate return, by-value record classification, or
cross-language exception frame is admitted. Exact ELF symbol bytes and visibility
must be declared and audited; imports name one external C symbol, while generated
exports use a versioned `zryna_c_v0_e_` prefix plus a checked logical name to avoid
the existing `zryna_v1_e_` namespace. Exact and ASCII-case-folded collisions with
exports, runtime helpers and linked foreign definitions reject before emission.

## Foreign resource declaration

Each operation must declare its library identity and exact symbol; parameter and
result C types; `nullable` or `non-null` for each pointer; byte length source and
maximum for every buffer; direction (`in`, `out`, `inout`); encoding (`bytes` or
well-formed UTF-8); access (`read` or `write`); owner before and after success;
borrow end; exact release operation and its matching allocator/library identity;
and success, recoverable error, trap and process-failure outcomes. Missing or
contradictory fields reject at declaration verification, not at an unsafe call site.
The library's documented contract is an input for a reviewed binding; it cannot be
established merely by matching C function types.

| Resource form | Owner and lifetime | Required cleanup and validation |
| --- | --- | --- |
| Borrowed input `(const uint8_t *p, size_t n)` | Zryna wrapper retains its owned value and lends stable bytes only for the synchronous call; C may not retain `p`. | `n` is byte length, checked before conversion to `size_t`. `(NULL, 0)` is allowed only if declared; `(NULL, n>0)` rejects. UTF-8 mode validates the complete byte sequence; no terminator is implied. |
| Borrowed output `(uint8_t *p, size_t capacity)` | Caller owns initialized storage and C may write only during the call. | An explicit reported written length must be `<= capacity`; a larger length is boundary failure before reading bytes. Partial initialization is tracked; failure never exposes uninitialized bytes. |
| C-owned returned `(uint8_t *p, size_t n)` | The named C library owns allocation until the wrapper accepts the result; after acceptance, the wrapper owns the release obligation, not the allocation itself. | A non-null allocated result creates the release obligation before length or encoding validation. Check pointer/length/nullability, max size and encoding before copying to a private Zryna value or retaining as an opaque foreign resource. Call the declared same-library release exactly once on every post-allocation exit. Never pass it to the Zryna runtime allocator. |
| Opaque `struct T *` handle | The declaration names one library-specific handle kind and whether the successful call creates, borrows, or consumes it. The wrapper tracks one live owner token per created handle. | Only the matching declared release consumes a live owned token, once. Null creation and failed operations transfer no ownership unless a separately reviewed API contract explicitly states a returned partial resource. No field access, arithmetic, cast across handle kinds, or forged integer-to-pointer value. |

A transferred input becomes foreign-owned only after a declared successful call result;
on a recoverable error it remains with the caller. Since arbitrary C may mutate or
consume before reporting failure, a transfer signature is admissible only with a
reviewed library guarantee of failure atomicity or an explicit compensation path.
Otherwise use a copy or keep the binding raw and unavailable to a safe wrapper.
If C reports a malformed pointer or length, safe cleanup is possible only when the
library contract guarantees that the reported pointer is releasable even on malformed
metadata; otherwise the wrapper reports host failure without dereference or guessed
`free`, and cannot claim leak-free recovery for that defect.
If a call returns several resources, validation and cleanup track each initialized
slot in declared order; failure or later conversion failure releases the accepted
prefix in reverse order. A release function must be infallible under its documented
preconditions, or its failure is a host/process failure with the resource marked
unresolved, never a successful language result. Repeated release, wrong allocator,
wrong library, stale handle and use after transfer are verifier or wrapper failures;
they never become a second C `free` call.

Raw foreign bindings expose C's documented preconditions and require an explicit
unsafe call boundary. Their signatures and metadata are verified, but memory safety
of arbitrary foreign code is not proved. A reviewed safe wrapper must additionally
prove all length arithmetic, pointer validity, alignment, aliasing, initialization,
null handling, UTF-8 conversion, owner transitions, cleanup on every exit and error
mapping for its exact library version. It cannot expose a naked pointer or a borrowed
view beyond the synchronous call. Zryna `String`, `Vec`, `Shared`, `Weak`, aggregate
layouts, runtime handles and helper symbols stay private; crossing uses a checked
copy or a distinct foreign handle. No C code may free or retain their internal
storage. Existing internal ownership-runtime status codes do not become C FFI codes.

## Failure and execution boundary

An expected C status is a recoverable result only when the operation declares the
exact status domain and establishes unchanged inputs plus the specified initialized
outputs for that status. A Zryna language trap (for example, a checked wrapper length
limit) follows verified cleanup and retains its trap identity. An invalid foreign
result, allocator mismatch, unknown status, cleanup failure, symbol mismatch, or
violated resource invariant is a host/ABI failure and must not be relabeled as a
language trap. C signal, abort, timeout, loader failure and nonzero harness exit are
process failures, never scalar returns. A corrupt or crashing in-process C library
cannot promise that cleanup runs; tests must report this limit rather than claim a
sandbox or recovery. Cross-language unwinding in either direction is forbidden: C
and Zryna entry shims must contain their own errors and return declared statuses.

V0 has **no callbacks, function pointers, C-held Zryna borrows, worker threads,
thread-affine handles, or reentrant entry**. A foreign call executes synchronously
on its invoking thread and must not call back into Zryna. An export invoked by a C
client is a fresh non-reentrant entry and must not be invoked concurrently until a
separate threaded runtime ABI specifies synchronization. Libraries requiring callback
lifetime, thread affinity, TLS interaction, recursive entry or concurrent release
reject in v0. Also excluded: varargs, unions, bitfields, flexible arrays, complex
by-value records, C++ object ABI, Rust native ABI, arbitrary by-value structs,
cross-language unwinding, and dynamically inferred library search.

## Representative C fixture and expected observations

The future fixture is a tiny versioned C library with `int32_t add(int32_t,int32_t)`;
its wrapper checks the mathematical sum fits `int32_t` before the call and traps with
verified cleanup on overflow. The library itself receives only admitted inputs. It also has
`int32_t sum_bytes(const uint8_t *, size_t, int32_t *out)`. The latter returns status
`0` and writes an initialized signed result for `n <= 4096`, status `1` for a larger
length, and leaves `out` untouched on error. It accumulates in a checked wider
integer and checks the result against `INT32_MAX` before the narrowing assignment;
any future bound that permits overflow must return a distinct declared range status
without writing `out`. No unsigned result is silently converted into Zryna `i32`.
The incomplete `struct fixture_handle` is created by
`int32_t fixture_open(int32_t seed, struct fixture_handle **out)`, queried by
`int32_t fixture_read(struct fixture_handle *, int32_t *out)`, and consumed by
`void fixture_close(struct fixture_handle *)`. `fixture_open` returns status `1`
without allocating or writing `out` for a negative seed; status `0` creates a
handle whose read result is the seed. Its contract fixes the allocator/release
pair, `NULL` and zero-length behavior, bounds, failure atomicity, and a trace
counter for each release. A separate generated export `zryna_c_v0_e_add` is
called by a C11 client using the generated exact header. The C client checks
`add(20, 22) == 42` twice as a full 32-bit result, without interpreting process
exit as the function result.

| Case | Expected future proof |
| --- | --- |
| `add(20, 22)`, `sum_bytes(NULL, 0)`, `sum_bytes([1, 2, 3], 3)`, and `open(7)`/read/close | Results `42`, `0`, `6`, and `7`; one handle release trace; no retained borrow after return. |
| `open(-1)` and wrapper sequence `open(7)` then `open(-1)` | Declared recoverable status `1`, no output on the failed call; the wrapper releases its first accepted handle exactly once and reads no uninitialized output. |
| `(NULL, n>0)`, over-limit length, invalid UTF-8 in UTF-8 mode, noncanonical Boolean shim | Rejected before unsafe call or result exposure with the declared boundary outcome. |
| Unknown status, written length above capacity, null non-null result, wrong-library handle or release, repeated close | Independent verifier/wrapper rejects; no second free; ABI/host failure where foreign code has already violated the contract. |
| Missing/renamed symbol, wrong ELF architecture or header width/alignment, undeclared library, wrong target or `all` selection | Rejected at declaration, artifact audit, link-input validation or profile selection as appropriate; no partial published artifact. |
| C attempts callback, retains borrowed pointer, unwinds across boundary, or requires worker-thread entry | Signature or reviewed library policy rejected; hostile execution is not classified as a recoverable language error. |

The reverse export test also checks wrong arity/type in the generated header as a
compile failure, invalid raw Boolean carrier as an entry failure before the body,
and repeated C-client calls with no leaked owner. C fixtures are deliberately
smaller than SQLite or a Rust wrapper so ABI and cleanup faults are visible alone.

## Bounded delivery and acceptance gates

| Stage | Prerequisite | Required evidence before advancing |
| --- | --- | --- |
| Design acceptance | Review this target table, resource policy and exclusions against #357; coordinate only native artifact fields with #361. | Signed-off contract decision, checked primary psABI/header comparisons, complete positive/negative case review; no support claim. |
| Prototype | Accepted contract; isolated fixture branch. | C header static assertions and callable scalar/buffer/handle examples on the exact Linux target; deliberately disposable prototype, no selector or distribution. |
| Profile and IR/MIR implementation | Accepted #357 native admissibility and exact accepted ownership/layout/profile gates. | Independent malformed declaration and hostile raw IR/MIR rejection; stable diagnostics; universal/JS/WASM/`all` refusal; no verified value from partial input. |
| Import/export and driver linking | Verified profile and exact #361 native artifact input decisions for artifact-backed linking. | Closed symbols, relocations, header identity, toolchain and target audits; create-only publication; scalar/buffer/handle and reverse C-client results. |
| Safe wrappers and later library proof | Audited raw boundary, reviewed library-specific ownership contract. | Injected allocation failure, partial failure, wrong allocator/library, null/length/UTF-8, repeated release, leak and sanitizer evidence; SQLite and Rust-through-C-shim each receive separate proof. |
| Per-platform conformance | Complete Linux fixture and wrapper matrix. | Linux x86-64 CI plus independent malformed and fault tests; each additional OS/ABI gets a new table, header comparison and separate evidence before activation. |
| Public activation | Accepted implementation, conformance, docs and release review. | Explicit selector/profile migration and published support decision; `specified`, `prototype`, `conformance-passed`, `publicly-supported` tracked separately. |

No stage changes the historical digest-pinned inventories or M3 gates. FFI completion alone
does not establish stable 1.0 compatibility or native frontend replacement.

## Decisions for reviewer agreement

- **Boolean C surface:** v0 proposes a `uint32_t` checked shim and excludes direct
  `_Bool`. A later direct `_Bool` option needs independent narrow-width call/result
  validation and a matching header/fixture proof; it cannot reuse the 32-bit public
  scalar carrier by assertion.
- **Export namespace:** `zryna_c_v0_e_` avoids the existing scalar namespace. The
  reviewer may choose another fixed prefix before implementation, but exact symbol
  collision and version tests remain required.
- **Foreign allocation:** v0 allows only a named same-library release obligation.
  Adoption into private Zryna storage is a checked copy. A zero-copy adoption option
  needs a new allocator and failure/cleanup proof and is outside this v0 draft.
- **Fault containment:** in-process C crashes remain process failures. Promising
  recovery from a hostile library requires a separately specified process-isolation
  boundary, not a wider metadata declaration.
