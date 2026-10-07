# Zryna JavaScript backend

Direct JavaScript lowering from sealed M1 and M2 verified programs.

The internal M3 `emit_data_ownership` entrypoint additionally accepts only a sealed
`DataOwnershipV1` program and its matching ownership-runtime ABI. It emits bounded deterministic
self-contained ESM with private aggregate/String/Vec/borrow/Shared/Weak helpers, explicit release
transitions, failure cleanup and no ambient capability. See `docs/M3_TARGET_BACKENDS.md`.
Its String helper retains a 64 MiB target allocation budget: an otherwise valid larger request is
`ALLOCATION`, while checked arithmetic or exceeding the universal 2,147,483,647-byte maximum is
`CAPACITY`.

The backend consumes only sealed function views and uses each export's sealed scalar ABI
JavaScript name. It emits a deterministic ECMAScript module with LF line endings and a final
newline. One local temporary is emitted for every canonical arena expression, in arena order,
followed by a validated return of the verified body. Emission is iterative and linear in the
verified IR size. Current `i32` addition is rendered as `(left + right) | 0` to preserve signed
wrapping 32-bit behavior.

Every public `I32V1` function checks exact arity and accepts only primitive JavaScript Numbers that
are finite integral signed 32-bit values and are not negative zero. Parameters and results are
validated without coercion. Stable backend diagnostics guard impossible verified-profile states.
Host-boundary failures use stable scalar ABI codes; carrier cases are exercised by the shared ABI
fixture and exact arity by driver integration.

The private Boolean carrier validator accepts only primitive JavaScript Booleans and is tested
against the shared scalar ABI fixture. It does not enable Boolean source: `bool` remains rejected
by the current Universal IR verifier until every active universal backend implements that profile.
Artifact publication and Node.js execution belong to `zryna-driver`, not this backend.

The separate internal `emit_control_flow` boundary accepts only the opaque verified
`control_flow_v1::VerifiedProgram`; raw M2 IR is not accepted. It emits every M2 scalar operation,
direct call, return, jump, and branch in canonical module/function/block order. Dense private names
come only from sealed identities. Entry exports use private typed wrappers plus scalar-ABI export
aliases, so public names cannot shadow implementation bindings. Jump and branch arguments are
copied through scratch slots before target parameters are assigned, preserving simultaneous loop
and merge edges. Conditions compare explicitly with `true` rather than using truthiness.

M2 multiplication uses a private exact low-32-bit decomposition instead of an ambient mutable
intrinsic. The same linear renderer first counts every exact byte and then emits into an exactly
reserved string, returning `ZRYNA-J2003` instead of an artifact beyond 32 MiB. Driver tests import
the public aliases and execute exact `i32` and primitive `bool` wrappers through the pinned,
bounded Node capability; backend tests cover control flow, calls, collisions, repeated bytes, and
exact rendered budget boundaries. M2 WebAssembly is a separate sealed internal backend and the
independently verified native MIR feeds the sealed internal M2 Linux x86-64 native object/link-run
backend. The public driver activates these sealed boundaries only under explicit
`--profile control-flow-v1`, publishes manifest v2 atomically, and is covered by the independent
[fixed-oracle three-target gate](../../docs/M2_CONFORMANCE.md). Default M1 behavior is unchanged;
authenticated public website and live-deployment closure remain Issue #57.


`generic_copy_v1::emit` accepts only the independently sealed successor immutable Copy program.
The [internal wire/source contract](../../spec/ir/GENERIC_COPY_WIRE_V1.md) documents admission,
private enum records, exact active payload transfers, scalar entry exports and remaining #416
obligations. Its complete-source fixture executes generic forwarding and both Option/Result
variants under pinned Node, including canonical i32/bool and exact-arity boundary rejection.
Its formatter rejects the first byte beyond 32 MiB with `ZRYNA-J2003` and preserves the bounded
buffer; a separate formatter test checks the exact byte ceiling without claiming source admission.
No existing backend entrypoint, driver route, runtime allocation helper or public profile changes.
