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
argument keys, substituted parameter/result types, source call site, private
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
