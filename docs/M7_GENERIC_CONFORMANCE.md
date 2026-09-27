# M7 bounded generic conformance proposal

Status: planning evidence map for [Issue #415](https://github.com/zryna/zryna/issues/415).
The [language proposal](../spec/language/BOUNDED_GENERICS_OPTION_RESULT_V1.md),
[IR boundary](../spec/ir/GENERIC_INSTANTIATION_V1.md),
[layout boundary](../spec/memory-model/GENERIC_ENUM_LAYOUT_V1.md) and
[ABI boundary](../spec/abi/GENERIC_VALUE_BOUNDARIES_V1.md) describe future
behavior. This table is a requirement, not an execution receipt.

The proposed [fixed bytes and digests](../spec/memory-model/generic-enum-layout-v1-fixtures.json)
are checked by `node --test tests/m7-generic-spec-fixtures.test.mjs`. This
validates candidate keys and records independently of any compiler backend.
The [instantiation fixtures](../spec/language/generic-instantiation-v1-fixtures.json)
pin mixed function/type key order and the finite versus expanding data cases;
`node --test tests/m7-generic-instance-spec-fixtures.test.mjs` checks their
candidate key bytes and ordering.

| Owning gate | Required positive and negative evidence |
| --- | --- |
| Syntax/provider | Explicit one/two-argument functions and nominal declarations; invalid bound, malformed application, omitted/extra arguments and v4 unchanged rejection; two providers agree on source-faithful spans |
| Semantics | Cross-module same-key deduplication, distinct closed identities, mixed function/type key order, finite `Box<Box<i32>>` admitted, same-key `Node<T>` through Vec admitted, generated `Nest<Vec<T>>` expansion and function recursion rejected, opaque-bound body checking; invalid operation on `T` and wrong argument |
| Resource | Exact and first-extra for 2 parameters/arguments, 4,096 generic functions and 4,096 closed generic data instances including Option/Result, 65,536 distinct ordered instance-edge pairs (non-generic roots count only as edge origins; duplicate occurrences count once), depth 64, key bytes 4,096, inherited 65,536 types and 256 diagnostics; checked overflow and pristine replay |
| IR | Valid closed instance calls and both standard enum variants; independent forged ID, key, substitution, payload, branch and cleanup rejection |
| Layout | Both storage targets and fixed size/offset fixtures; independent reproduction of proposed type-key bytes/SHA-256 and full Option, Box, Choice and Result record digests; changed family tag, argument, ordinal, target or digest rejection; synthetic arithmetic limits |
| ABI | Internal calls accepted; public generic or Option/Result signatures rejected; no accidental export or host carrier |
| Ownership | Copy and owned payload construction, by-value/borrowed exhaustive matching, inactive payload untouched, exactly once cleanup on return and every controlled failure path |
| Target | Fixed scalar oracle for all variants and nested owned values on JavaScript, core WebAssembly and admitted Linux native target; equal trap and logical drop/release trace under fault injection |

Each implementation slice records its exact revision, platform, command, exit
status and executed/ignored counts. Documentation/schema, fixed-fixture,
ambiguity, digest and dependency checks belong to specification review.
Authenticated source and independent hostile IR are separate evidence classes.
Runtime fault traces and three-target execution are later gates and cannot be
inferred from a passing documentation check. No historical digest-pinned
inventory or current public support statement is changed by this plan.
