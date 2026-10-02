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
| Resource | Exact and first-extra for 2 parameters/arguments, 4,096 generic functions and 4,096 closed generic data instances including Option/Result, 65,536 distinct ordered instance-edge pairs (non-generic roots count only as edge origins; duplicate occurrences count once), canonical type-application depth 64 including M3 containers, key bytes 4,096, inherited 65,536 types and 256 diagnostics; checked overflow and pristine replay |
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

## Complete proposed acceptance packet

The [decision catalogue](../spec/language/BOUNDED_GENERICS_DECISIONS_V1.md)
completes exact admitted/excluded source forms, source-template visibility versus
executable exports, Copy/Clone, shared/exclusive match, failure disposition,
diagnostic spans/order, and #357 compatibility. All companion proposals are one
review unit; their status is proposed/unaccepted until explicit acceptance.

| Issue #415 requirement | Normative decision | Specification-review evidence | Later #416 obligation |
| --- | --- | --- | --- |
| Every admitted/excluded form | Language sections 1/3 and complete decision catalogue | Closed review schema; positive/rejected source-form catalogue; pinned TS6 parser/AST ambiguity checks | Versioned DTOs, hostile syntax, native/bootstrap semantic/diagnostic parity |
| Stable identity/order/recursion | Language section 2 and IR proposal | Existing mixed-key/depth fixtures plus diamond-module packet with exact function/data keys; canonical key decoder mutations and replay | Actual complete instance graph, substitutions and independent hostile IR |
| Enforceable exact/first-extra bounds | Language section 2 and diagnostic/resource decisions | All seven numeric reference boundaries, duplicate ordered-pair synthetic graph, canonical depth/key-byte boundary fixtures, checked arithmetic | Real producer/verifier terminal rejection, inherited source/type/IR/layout limits and no partial result |
| Option/Result representation | Layout proposal and ABI boundary | Scalar records plus owned/nested full record/digest fixtures on both storage targets; closed layout-review schema and mutations | Independent sealed layout authorities, forged key/record/ordinal rejection |
| Match/ownership/failure | Decision catalogue and IR/ABI proposals | Fixed all-variant observations, inactive-payload, nested ownership, constructor/match/payload fault and borrowed cleanup requirements | Executed fixed three-target scalar/trap/release observations and fault injection |
| M3/#357 compatibility/dependency-ready plan | Decision catalogue delivery table and ROADMAP M7 bullet | Four inherited contract SHA-256 pins and reviewed dependency table | Unchanged existing bytes/diagnostics; future separate language/profile/manifest admission |

Run the complete light review gate with pinned Node.js 22.22.1 after frozen install:

```sh
node --test tests/m7-generic-spec-fixtures.test.mjs tests/m7-generic-instance-spec-fixtures.test.mjs tests/m7-generic-review-spec.test.mjs tests/m7-generic-resource-spec.test.mjs tests/m7-generic-owned-layout-spec.test.mjs tests/m7-generic-key-spec.test.mjs
pnpm docs:check
pnpm structure:check
```

The source catalogue contains syntax fragments with their names/types/owners
assumed authenticated in the documented context. Its admit/reject/code fields
are fixed semantic decisions, not observed compiler diagnostics. The module
packet supplies complete source files and fixes future `i32:14`, one imported
function instance, one imported data instance and forbidden generic ABI entries.
The tests check syntax, reference encodings, arithmetic, schema closure and
oracle preservation; they do not type-check, instantiate or run those programs.
The cleanup trace vocabulary is logical `release:<owner>` and `end-loan:<role>`;
it excludes target addresses, padding and runtime-specific handles. Later gates
must reproduce these logical events, including no event for inactive payloads.

Repository-required preflight/M0 and supported-platform hosted checks remain
submission/integration requirements. They require the coordinated build slot;
passing this light packet does not waive them or establish runtime acceptance.
