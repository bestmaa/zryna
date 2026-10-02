# Generic instantiation verified IR v1 proposal

Status: specified candidate for [Issue #415](https://github.com/zryna/zryna/issues/415).
This is a future versioned extension; current `DataOwnershipV1` verified IR must
continue rejecting user generics, `Option` and `Result`.

The [language contract](../language/BOUNDED_GENERICS_OPTION_RESULT_V1.md) owns source
meaning and canonical closed instance keys. Function keys use tag `40`, source
function roots use `41`, and closed data types use the disjoint layout key tags;
compiler-owned Option/Result have no source declaration index. The IR authority
receives the exact authenticated module graph, a sorted generic-function
inventory and a separately sealed closed type universe. It alone seals
backend-consumable function-instance IDs. A raw producer may
claim IDs but cannot make them authoritative.

Each verified function instance carries its declaration identity, ordered type
argument keys, substituted parameter/result types, all referring source call sites, private
symbol identity and full ownership/drop plan. Every call names one existing
instance ID and has exact arity, argument types, result type and ownership
transfer. No type parameter, unresolved application, type erasure, dynamic
dispatch, implicit clone or target symbol is permitted in verified executable IR.
Generic nominal values name one complete closed type ID. The verifier recomputes
the substitution and complete key from authenticated declarations and rejects a
claimed key, ID, count, type, call target or owner that differs.

`Option` and `Result` construction records the closed family type, fixed variant
ordinal and optional exact payload type. A match records the same closed family,
one successor per variant, exact active-payload binding and drop/borrow transfer
on each edge. The verifier rejects missing/duplicate successors, a forged
discriminant, inactive-payload use, an uninitialized payload, a borrow escaping
its region, or cleanup that omits or repeats an active owner. Runtime-invalid
discriminants are rejected at any future authenticated aggregate boundary before
constructing a verified value.

Resource preflight must bound the complete substituted graph, dense IDs, edges,
drop actions and diagnostics before sealing. The exact candidate ceilings are in
the language contract; existing IR/ownership ceilings remain effective. Exhaustion
returns no partially verified module. Traversal and diagnostic selection use
canonical key order and source spans, independent of hash-map or backend order.

Conformance needs an authenticated source-to-IR fixture for two distinct
instantiations of one function and one nominal type; separate hostile raw-IR
mutations of key, substitution, target ID, variant, payload and cleanup; and
exact/first-extra, overflow and replay fixtures. A valid producer output alone
does not prove the verifier boundary. The eventual implementation must freeze
exact new IR tags and diagnostic codes before executable use.

The successor's logical operations are `ClosedGenericCall(instanceId,
arguments)`, `ClosedEnumConstruct(typeId, ordinal, payload?)` and
`ClosedEnumMatch(typeId, scrutinee, mode, successors)`, where mode is exactly
`value`, `shared-borrow` or `exclusive-borrow`. These are logical contract names,
not additional tags in current raw IR. Each successor contains ordinal, optional
payload binding/type, exact result type, source span and verified owner/loan
transfers. Bindings and successors occur in ordinal order; execution selects
only the discriminant's edge. The constructor has zero payload operands for
none and exactly one for every payload variant. Generic-call arguments retain
source evaluation order. All reachable and unreachable claimed blocks are
verified under the existing canonical graph rules; no orphan claim can hide an
invalid owner, variant or new instance.

The verifier binds the whole complete instance inventory, declaration and
SourceMap identities, both successor layout fingerprints, selected future
language contract and ownership-runtime declaration identity. It rejects omitted
or extra inventory members, duplicate keys, nondense/sorted IDs, wrong source
provenance, invented private symbols, and graph authority from another compilation.
One deduplicated instance can have multiple call sites; call-site provenance is
not part of its key or ID. Physical wire tag allocation is a separate new-version
serialization gate in #416; no producer can execute these logical operations
until that schema and independent hostile decoding tests are frozen.
