# Native C v0 design acceptance candidate

Review scope: [#364](https://github.com/zryna/zryna/issues/364), before the
implementation in [#417](https://github.com/zryna/zryna/issues/417).
The [native C v0 draft](NATIVE_C_INTEROP_V0.md) remains specified-only. This
candidate records the exact design decisions and review cases; it grants no
foreign declaration, import, export, wrapper, linking or public profile.
Maintainer sign-off is pending.

## Target authority checked

The primary [AMD64 psABI source](https://gitlab.com/x86-psABIs/x86-64-ABI/-/blob/e1ce098331da5dbd66e1ffc74162380bcc213236/x86-64-ABI/low-level-sys-info.tex)
at `e1ce098331da5dbd66e1ffc74162380bcc213236` was checked against the
draft table and calling sequence. Its **Scalar Types** table gives the LP64
widths and alignments for `_Bool`, `char`, the signed and unsigned standard
integer widths, and object pointers. `stdint.h` fixed-width aliases and
`size_t` also require the selected target's header check: the psABI table does
not make their spelling a Zryna language mapping. The source's **Classification**
and **Passing** sections classify integers and pointers as INTEGER and assign
six argument registers in order: `%rdi`, `%rsi`, `%rdx`, `%rcx`, `%r8`, `%r9`.
Further INTEGER arguments use the ABI stack area. **Returning of Values**
assigns a single admitted INTEGER result to `%rax`. **The Stack Frame** requires
the call boundary's 16-byte alignment; **Registers and the Stack Frame** names
the callee-preserved registers. The psABI explicitly leaves excess bits of
narrow INTEGER arguments and results unspecified; low-width validation is
mandatory. This comparison is an ABI authority check, not an executed Zryna
call.

The inert [candidate C header](../../tests/native-c-abi-v0/candidate.h) and
[compile-time checks](../../tests/native-c-abi-v0/header_contract.c) are
review inputs for `x86_64-unknown-linux-gnu` LP64. They check byte width,
`sizeof`, `_Alignof`, signedness, unsigned `size_t`, and exact C function
pointer types for the proposed scalar, buffer, opaque-handle, release and
reverse-export declarations. They do not test ELF symbol binding, runtime
calling sequences, ownership, generated headers, or a Zryna import/export.
`int32_t` is a typedef compatible with C `int` on the checked host, so C type
compatibility alone cannot enforce which spelling a verified foreign
declaration used. That distinction must be retained and audited as explicit
ABI metadata.

## Decisions submitted for agreement

| Decision | Candidate v0 rule | Rejection or follow-up boundary |
| --- | --- | --- |
| C Boolean | Use only a `uint32_t` shim carrying exact `0` or `1`, checked on entry and result. It is an exception for the Boolean carrier, not general unsigned-language support. | Reject direct `_Bool` signatures and all other shim values. A future `_Bool` lane needs its own 8-bit proof. |
| Export symbols | Generate `zryna_c_v0_e_` plus a checked logical name, separate from `zryna_v1_e_`; bind exact ELF bytes and visibility. | Reject exact and ASCII-case-folded collisions against exports, runtime helpers and foreign definitions before emission. Never fall back to another symbol. |
| Foreign allocation | Pair each accepted allocation with its exact named same-library release; copy validated bytes into private Zryna storage. | Reject wrong allocator/library, repeated release, and zero-copy adoption. A malformed result is safely releasable only with the library's explicit guarantee. |
| Process faults | Treat signal, abort, timeout, loader failure and nonzero harness exit as process failures. An in-process crash has no cleanup or recovery guarantee. | Do not report a language trap, recoverable status, or sandbox containment for a crashed foreign library. |

The status domain, nullability, length maximum, encoding, owner transition,
borrow end, release identity and failure atomicity are per-operation reviewed
inputs. A matching C type is insufficient to establish these promises. The
`size_t` lane is an explicit 64-bit unsigned ABI carrier after checked length
conversion. A generic `uint32_t` value, C `long`, C `_Bool`, and internal
String/Vec/Shared/Weak layouts remain inadmissible by inferred source spelling.
No callbacks, worker threads, thread-affine handles, reentrant entry,
variadics, by-value records or cross-language unwinding are admitted.

## Candidate fixture operation policies

`fixture-c-v0` is the proposed logical identity for the five imported C
operations in the candidate header, not a filename, acquired artifact or
linker grant. Its one handle kind is `fixture-c-v0/fixture_handle`; its
allocator identity is `fixture-c-v0/fixture_open`, paired only with release
`fixture-c-v0/fixture_close`. The generated reverse export belongs to the
output artifact, whose concrete identity the driver must validate under the
accepted #361 native inputs. Every pointer below is non-null unless the row
explicitly admits `NULL`; every call is synchronous and forbids retention,
callbacks, reentry, threading and cross-language unwind. Abort, signal,
timeout and loader failure remain process failures for every operation.

| Exact C symbol | Input, owner and borrow end | Result, status and release obligation |
| --- | --- | --- |
| `add(int32_t, int32_t)` | No pointers or resource transfer. Both inputs are exact signed 32-bit values. | Direct wrapping `int32_t` sum for every input; no status domain, allocation or release. The C body must use unsigned modulo arithmetic and reconstruct an in-range signed result, never C signed-overflow arithmetic. |
| `sum_bytes(const uint8_t *, size_t, int32_t *)` | Caller owns stable input bytes and the `out` slot through return. Input borrow ends at return. `(NULL, 0)` is admitted; `(NULL, n>0)` rejects. Bytes mode has no UTF-8 requirement; C cannot retain the pointer. A safe wrapper checks that the byte count fits `size_t` and is at most `4096` before C entry. | Raw fixture status `0` writes one initialized `int32_t` sum for `n <= 4096`; a direct raw call with valid input and `n > 4096` returns status `1`, leaving `out` untouched and input unchanged. The safe wrapper never calls C for that length. The caller reads `out` only after `0`. Unknown status or a write on `1` is host/ABI failure. No allocation or release. |
| `fixture_open(int32_t, struct fixture_handle **)` | Caller owns the `out` slot. No input handle or transfer. | Status `0` writes one non-null newly allocated `fixture-c-v0/fixture_handle`; its release obligation passes to the wrapper immediately. Status `1` for a negative seed allocates nothing, leaves `out` untouched and preserves earlier handles. Unknown status, null success or partial allocation on `1` is host/ABI failure. The only release is `fixture_close`, exactly once. |
| `fixture_read(struct fixture_handle *, int32_t *)` | A live `fixture-c-v0/fixture_handle` is borrowed without transfer until return; caller owns `out`. Null, stale, transferred or wrong-library handles reject before C entry. | Status `0` writes the seed to `out`; there is no recoverable error status in this fixture. Unknown status or unwritten `out` on `0` is host/ABI failure. The handle stays owned and later needs its one `fixture_close`; `out` is read only after `0`. |
| `fixture_close(struct fixture_handle *)` | Consumes one live owned `fixture-c-v0/fixture_handle`; no other allocator or library may supply it. Null, repeated or wrong-kind close rejects before C entry. | `void` release is infallible under these preconditions and records exactly one release. A release fault is host/process failure with the obligation unresolved, never a successful return or second free. |
| `zryna_c_v0_e_add(int32_t, int32_t)` | Generated fresh non-reentrant export with exact signed 32-bit carriers; no pointers, foreign library, allocation, resource transfer or host effects. | Direct full-width wrapping `int32_t` sum for every input. There is no controlled arithmetic-overflow trap or status domain for this pure body. Process faults remain process failures, not scalar results. |

The `sum_bytes` status-`1` and `fixture_open` status-`1` guarantees are the
fixture's proposed failure-atomicity claims; a real library requires its own
documented proof. An in-process C contract violation does not make arbitrary
malformed pointers safely releasable. This table fixes design inputs only;
independent declaration, wrapper, linker and execution checks remain later
acceptance work.

The accepted [`ControlFlowV1`](../language/CONTROL_FLOW_MODULES_V1.md)
addition rule is signed two's-complement modulo `2^32`, with no arithmetic
trap; [`DataOwnershipV1`](../language/DATA_OWNERSHIP_V1.md) inherits it.
The direct-result export is admissible for this pure total body. A genuinely
fallible future export needs its own declared status and result channel with
exact controlled-trap identity, no output on failure and no cross-language
unwinding; the direct `add` signature does not silently supply that channel.

## Review cases and future evidence owner

These are acceptance cases, not test results. “Before call” refers to
declaration/profile verification or a safe wrapper; a foreign violation found
after a call is a host/ABI failure. The C header check above covers only the
type rows indicated.

| Case | Required observation | Evidence owner after acceptance |
| --- | --- | --- |
| `add(20, 22)` and reverse `zryna_c_v0_e_add(20, 22)` twice | Full signed 32-bit result `42` each time; generated header matches the export. | Import/export execution and C client |
| `add(INT32_MAX, 1)` and reverse export with the same inputs | Both return `INT32_MIN` through defined wrapping behavior; C fixture and client never perform signed-overflow arithmetic. | Import/export execution and C client |
| `sum_bytes(NULL, 0)` and `[1, 2, 3]` length `3` | Accepted when null-zero is declared; results `0` and `6`; no retained borrow. | Safe wrapper and C fixture |
| `open(7)`/read/close | Read `7`; one same-library release trace. | Resource wrapper and C fixture |
| `open(-1)` after an earlier accepted handle | Status `1` leaves the failed `out` untouched; earlier handle released once; uninitialized output unread. | Injected partial-failure fixture |
| `(NULL, n>0)`, safe-wrapper length above maximum | Rejected before the unsafe call or result exposure, with the declared boundary outcome. | `sum_bytes` safe wrapper |
| Invalid UTF-8 in a separately declared UTF-8 wrapper | Complete byte sequence rejected before the unsafe call or result exposure; this is not a `sum_bytes` bytes-mode case. | Later UTF-8 wrapper |
| Direct raw `sum_bytes` with `n > 4096` and valid input | C fixture returns status `1`, leaves `out` untouched and changes no input; this tests the C status contract, not the wrapper precheck. | Raw C fixture |
| Boolean shim `0`, `1`, `2`, and `UINT32_MAX` | First two accepted; others rejected before body or result exposure. | Import and export shims |
| Unknown status, written length above capacity, null non-null result | Host/ABI failure after foreign violation; no uninitialized read or guessed cleanup. | Independent hostile-result tests |
| Wrong-library handle, allocator or release; repeated close | Rejected before a second free; never treated as recoverable success. | Independent verifier and wrapper tests |
| Missing/renamed symbol, wrong ELF target, undeclared library | Rejected at artifact/link-input validation; no partial publication. | Object audit and driver |
| Native foreign requirement with universal, JavaScript, WebAssembly or `all` selection | Rejected before backend emission, with no fallback target. | #357 profile verification and driver |
| Callback, retained borrow, worker-thread or reentrant requirement | Signature or reviewed-library policy rejected. | Declaration verifier and binding review |
| C abort/signal/timeout or prohibited unwind | Process failure; no claimed in-process recovery or guaranteed cleanup. | Isolated process harness |

The later #417 proof must additionally inject allocation failure, validate
cleanup of an accepted resource prefix in reverse order, exercise a failed
release and invalid raw IR/MIR independently of the producer, and run the
linker, generated-header, Linux execution and sanitizer evidence on the exact
revision. The #361 native appendix remains provisional until the ABI decisions
above are accepted; its artifact identities do not grant a linker input.

## Local design-evidence commands

From `tests/native-c-abi-v0` on Linux x86-64 LP64, compile with GCC in C11
syntax-only mode:

```sh
cc -std=c11 -Wall -Wextra -Werror -pedantic -fsyntax-only header_contract.c
cc -std=c11 -Wall -Wextra -Werror -pedantic -fsyntax-only -DREJECT_EXPORT_ARITY header_contract.c
cc -std=c11 -Wall -Wextra -Werror -pedantic -fsyntax-only -DREJECT_EXPORT_BOOL_WIDTH header_contract.c
cc -std=c11 -Wall -Wextra -Werror -pedantic -fsyntax-only -DREJECT_BUFFER_LENGTH_WIDTH header_contract.c
cc -std=c11 -Wall -Wextra -Werror -pedantic -fsyntax-only -DREJECT_HANDLE_POINTER_LEVEL header_contract.c
```

The first command must succeed. Each `REJECT_*` command must fail for a
conflicting declaration, showing that a changed arity, `_Bool` result,
truncated buffer length or wrong handle pointer level is noticed by the C
compiler. These deliberate failures are not runtime conformance or proof of
the verifier's eventual rejection paths. A future generated header must be
compared against the reviewed exact library header and tested with a real C
client before import/export claims are made.
