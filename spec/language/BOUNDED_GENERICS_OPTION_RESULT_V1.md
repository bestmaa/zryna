# Bounded generics, Option, and Result v1

Status: **specified proposal for Issue #415; no public or compiler activation**. The
syntax, diagnostic codes, binary key and limits below are candidate decisions for
review. Acceptance of this document alone does not change `DataOwnershipV1`, syntax
protocol v4, verified IR, aggregate layout v1, scalar ABI v1, any target backend, or
the public profile. A future protocol and implementation issue must freeze and test
the candidate encoding before executable use.

Companion proposals cover the [verified IR](../ir/GENERIC_INSTANTIATION_V1.md),
[closed layout](../memory-model/GENERIC_ENUM_LAYOUT_V1.md),
[ABI boundary](../abi/GENERIC_VALUE_BOUNDARIES_V1.md), and
[conformance/resource evidence](../../docs/M7_GENERIC_CONFORMANCE.md).

## 1. Authority and admitted forms

This proposal extends the [M3 ownership contract](DATA_OWNERSHIP_V1.md) without
reinterpreting existing values. Names resolve through the authenticated final M2
module map. Source type checking, instantiation and ownership checking belong to
Zryna semantics; a provider reports source syntax and spans only. The mandatory IR
verifier independently checks closed types, call substitutions and cleanup plans.

The initial user-defined forms are top-level functions and nominal struct/enum
declarations with one or two invariant type parameters. A parameter occurs only in
value parameter/result types, fields, variant payloads, and admitted nested M3
containers. No locally declared generic functions or types, methods, closures, aliases, interfaces
other than the nominal markers, or generic exports at scalar ABI v1 are admitted.
All function parameters and results retain explicit types. Generic values must be
fully instantiated before layout, IR sealing or code generation. Bare generic names
are never runtime values.

The proposed TypeScript-compatible source spelling is:

```ts
export interface Box<T extends ZrynaValue> extends ZrynaStruct {
  value: T;
}

export interface Choice<T extends ZrynaValue, E extends ZrynaValue> extends ZrynaEnum {
  ok: T;
  err: E;
}

function identity<T extends ZrynaValue>(value: T): T {
  return value;
}
```

`ZrynaValue` is a compiler-reserved bound marker, not an ordinary interface, an
object type, an implicit conversion or an erasable runtime trait. Its candidate
admission rule is recursive over complete, storable types: the M3 scalars and
`String`; closed M3 and user generic nominal structs/enums whose fields or
payloads satisfy the rule; `Option<T>` and `Result<T,E>` with admitted arguments;
and fixed arrays, `Vec`, `Shared` and `Weak` with admitted arguments. Existing
layout, recursion and container-element restrictions still apply to each closed
instance. The finite type graph is checked with visited closed keys, so a legal
`Node<T>` through `Vec<Node<T>>` can satisfy the bound without infinite unfolding.
It excludes `unit`, borrows, unsized/incomplete forms, host resources and an
unbound type parameter. A type parameter explicitly declared with this bound
may itself be used wherever this bound is required. There is no bound
inference. The bound list is exactly `extends ZrynaValue`; multiple bounds,
constraints on generic applications, defaults, variance annotations and `keyof`
are excluded. A generic body is checked once with opaque type parameters; it may
move, borrow, return, store or pass `T` according to existing ownership rules, but
cannot assume `T` is Copy, cloneable, indexable or a particular nominal type.
Specialization never makes an invalid generic body valid.

Generic nominal declarations retain the existing nonempty field and variant
requirements. Their source declaration identity remains `(ModuleId,
declaration-index)`. A closed instance additionally contains its ordered complete
type argument identities. `Box<i32>` and `Box<bool>` are distinct nominal types;
identical structures never make them interchangeable. By-value recursion is checked
after substitution, including a cycle that appears only for a particular argument.
An indirection terminates layout recursion but does not erase the type argument.

At a function call or nominal construction, type arguments appear after the
callee and immediately before the argument list: `identity<i32>(7)` and
`Box<i32>({ value: 7 })`. Standard enum constructors use a member call with
type arguments after the member name: `Option.some<i32>(7)` and
`Result.err<i32, String>(message)`. The type annotations retain the ordinary
`Option<i32>` and `Result<i32, String>` form. An instantiation expression
followed by property access, such as `Option<i32>.some(7)`, is excluded because
it is not TypeScript-compatible source syntax. The future provider must preserve
the callee/member and each explicit argument span without assigning its meaning.
Inference from arguments, expected results, or omitted generic arguments is
excluded. Importing a generic declaration follows the existing exact named-import
and module-closure rules. A source-level generic function name remains unique in
its module: overload sets and specialization are excluded. The source call graph,
including imported generic calls, remains acyclic. Instantiation cannot introduce
direct or mutual function recursion. A future syntax protocol must define distinct nodes
for parameter declarations, bound spans, closed type applications and explicit call
arguments; protocol v4 must reject these forms unchanged.

## 2. Closed instantiation and monomorphization

An instance key consists of the authenticated module ID, source declaration index,
declaration kind (function, struct or enum), ordered count of arguments and their
canonical complete type keys. Every nested key is length-prefixed. The proposal
uses the same primitive/container key categories as [aggregate layout v1](../memory-model/AGGREGATE_LAYOUT_V1.md),
but a generic nominal key additionally includes the ordered closed arguments.
The eventual layout contract must assign new tagged, unambiguous encodings; current
layout tags and fingerprints cannot represent these instances and must reject them.
Source spelling, alias path, traversal order, target and host address never enter
an instance key. Distinct instances with identical bodies do not merge.

Discovery starts from all admitted non-generic functions and fully closed data
types in the authenticated module graph. Walk declarations in ascending ModuleId
and source declaration index; within each body visit type annotations, expressions
and calls in source order. Enqueue each new closed instance by its canonical key,
then process the smallest pending key by unsigned bytewise order. Each instance
body is substituted once, checked, and added to a globally deduplicated inventory.
The final instance inventory is sorted by canonical key, then assigned dense
instance IDs in that order. A later backend may choose symbol spellings only from
these sealed IDs and verified keys; it may not discover new instances. The same
closed source graph gives the same IDs across targets and repeated builds.

Candidate compile-time ceilings are below. Every count uses checked arithmetic;
the exact-limit member is admitted and the first extra member is rejected before
producing a partial inventory or IR. Existing stricter source, type, layout, IR and
runtime limits still apply.

| Per authenticated compilation | Maximum |
| --- | ---: |
| Type parameters on one declaration | 2 |
| Explicit type arguments at one use | 2 |
| Distinct closed generic function instances | 4,096 |
| Distinct closed generic nominal instances | 4,096 |
| Total closed instantiation dependency edges | 65,536 |
| Maximum nested closed type application depth | 64 |
| Maximum key bytes for one closed instance | 4,096 |

An instantiation dependency edge is one distinct ordered pair of closed source
instance keys `(from, to)`: a substituted function body referring to a closed
generic function or nominal type, or a substituted nominal field/variant type
referring to a closed generic nominal, `Option` or `Result` instance through any
number of M3 container wrappers. Repeated occurrences of the same pair count
once. Scalar and nongeneric references do not count. A self-edge counts once.
The edge inventory is sorted by `(from key, to key)` before assigning IDs or
checking the limit. A synthetic fixture with 65,536 distinct pairs is accepted;
adding the lexicographically next pair is the first-extra rejection, even when
the same pair also occurs multiple times in source. Synthetic graph fixtures
exercise this budget independently of stricter source-size limits.

These ceilings do not increase M3's 65,536 fully instantiated type budget or
256-diagnostic ceiling. A bounded work queue and explicit stack implement discovery;
host call-stack depth and map insertion order cannot affect acceptance. Function
recursion is rejected on the source-level call graph, regardless of equal or
different type arguments. For data type expansion, a repeated *same closed key*
is deduplicated and terminates traversal; its by-value cycle is then separately
rejected by layout, while an indirection cycle such as
`Node<T> { next: Vec<Node<T>> }` remains legal. Repeating one generic nominal
declaration with *different* argument keys on a type-expansion path, as in
`Nest<T> { next: Vec<Nest<Vec<T>>> }`, is rejected before further expansion.
The diagnostic identifies the lexicographically smallest offending canonical
path. Thus legal finite recursive data and forbidden infinite expansion have
different outcomes, independent of the resource ceiling.

## 3. Option and Result

`Option<T>` and `Result<T, E>` are reserved, compiler-owned nominal enum families,
not user-definable or structurally compatible with a same-shaped enum. Their only
admitted arguments satisfy `ZrynaValue`. Constructor and match notation is a
proposed extension of the v4 enum forms:

```ts
const found: Option<i32> = Option.some<i32>(7);
const absent: Option<i32> = Option.none<i32>();
const outcome: Result<i32, String> = Result.ok<i32, String>(7);
const failed: Result<i32, String> = Result.err<i32, String>(message);

const score: i32 = match(found, {
  "Option.none": () => 0,
  "Option.some": (value) => value,
});
```

`identity(value)` is rejected because the type argument is omitted;
`Option.some<i32>(true)` is rejected for a payload type mismatch; and a match
containing only `"Result.ok"` is rejected as nonexhaustive. These are source
examples for a future provider, not protocol-v4 accepted input.

Variant ordinals are fixed: `Option.none=0`, `Option.some=1`,
`Result.ok=0`, `Result.err=1`. `none` has no payload; the others carry exactly one
value of the named type. There are no null/niche sentinels, truthiness conversions,
implicit success/failure propagation, exceptions, default values or hidden error
channel. `Result` is an ordinary value: `err` is not a trap, and constructing or
returning it runs no special control flow. Payload expressions evaluate once, left
to right. The selected payload is moved into the value; an unused argument is an
arity error. Existing explicit clone rules apply to a payload before construction.

`match` follows the existing complete enum match operation. Every arm is exact and
exhaustive, with no duplicate or unknown variant; a payload binding exists only
inside its selected arm. Matching evaluates the scrutinee once, tests its verified
discriminant and executes one arm. By-value matching transfers only the active
payload, then drops any active payload not transferred, exactly once. Borrowed
matching retains the owner and obeys current borrow regions; it cannot move from
the borrow. If an arm traps, its live values and the retained scrutinee follow the
existing reverse cleanup order. Inactive payload bytes are never read or dropped.
These requirements hold for Copy and owned arguments, nested containers and
indirection-recursive values.

Representation is a closed enum record using the existing four-byte discriminant,
payload alignment/maximum-size/offset formula and target-specific storage limits.
For example, `Option<i32>` is `8/4` with payload offset 4 on both current storage
targets. `Result<i32, bool>` is likewise `8/4` with payload offset 4. A `String`
payload gives target-specific sizes under the existing formula; the logical
variant, ownership and observation remain target equivalent. JavaScript stores a
private typed variant, never a public object shape. The new nominal key, sealed
layout record, fingerprint and IR type tags require a separately versioned layout
and IR contract; reusing v1 bytes without a new tag is invalid.

Scalar ABI v1 rejects all Option/Result parameters and results, including a generic
export that could instantiate to them. A later aggregate or component ABI must
independently specify carriers, validation, resource lifetimes and versioning.
WIT, JS/WASM adapters, native FFI and host grants cannot infer exposure from this
internal representation. In particular this proposal makes no #400 F1/F2 host
permission decision. Existing `upgradeWeak` keeps its two-successor operation;
it does not silently start constructing `Option<Shared<T>>`.

## 4. Failure, diagnostics and conformance gates

Malformed source, unknown/non-value arguments, missing or extra arguments, illegal
generic operations, unclosed types, forbidden recursion, invalid nominal identity,
wrong variant and nonexhaustive match are compile-time errors before backend
emission. These are proposed exact codes, awaiting maintainer acceptance and
future protocol/IR versioning; no current executable path may emit them:

| Proposed code | Owning rejection |
| --- | --- |
| `ZRYNA-D7001` | Invalid generic declaration form, bound or parameter list |
| `ZRYNA-M7001` | Missing, extra, unclosed or bound-violating type argument |
| `ZRYNA-M7002` | Operation unavailable for opaque bounded parameter |
| `ZRYNA-M7003` | Function recursion or expanding nominal instantiation |
| `ZRYNA-M7004` | Wrong standard variant or inexact/nonexhaustive match |
| `ZRYNA-M7005` | Generic or standard enum at a forbidden public ABI boundary |
| `ZRYNA-M7201` | Terminal instantiation resource exhaustion |
| `ZRYNA-I7001` | Unclosed, mismatched or forged verified-IR instance/variant/cleanup |
| `ZRYNA-L7001` | Invalid closed layout key, record, ordinal or fingerprint |
| `ZRYNA-L7201` | Terminal closed-layout resource exhaustion |

Malformed provider syntax must fail in a future versioned syntax verifier before
semantic codes apply; it cannot be retrofitted to v4. Selection follows
authoritative source location, then canonical instance key and complete numeric
tie-break data. Budget exhaustion is terminal and cannot return a partial sealed
program. No existing diagnostic code is reassigned by this proposal.

The implementation must provide at least these checked fixtures, with exact
accepted/rejected outcome and stable diagnostics recorded by its owning phase:

| Class | Required cases |
| --- | --- |
| Positive source | Explicit `identity<i32>` and `identity<String>`; cross-module imported `Box<i32>`; distinct `Box<bool>` identity; Option and Result construction and exhaustive match |
| Negative source | Omitted/extra type argument, unknown bound, borrow argument, generic export at scalar ABI, unbound-only operation such as `clone(T)`, duplicate/missing/wrong variant, nonexhaustive match, direct and expanding instantiation cycle |
| Ownership | Move once from each active owned payload, borrowed observation without move, nested owned cleanup, inactive payload untouched, trap in a later constructor operand or match arm |
| Layout and IR | Independent hostile generic-key/argument/ordinal/fingerprint mutations rejected; both target layouts; forged discriminant and wrong active payload rejected at a trust boundary |
| Resources | Exact and first-extra for every table ceiling and inherited M3 ceiling; checked overflow; no partial inventory, stale owner or cleanup plan after failure; repeat build replays same ordered keys and diagnostics |
| Cross target | Fixed scalar observations and controlled trap/cleanup traces for JS, core Wasm and admitted native target, including both Result variants and owned payloads |

Documentation/schema, fixed-fixture, ambiguity, digest and dependency checks are
required for the specification review. A later implementation needs authenticated
source-to-verified-IR evidence, independent hostile-input rejection, both storage
layout authorities, runtime fault injection and fixed-oracle target execution at
one exact revision. Those results cannot be claimed from this document.

## 5. Review decisions and implementation order

Before freezing syntax, reviewers must decide whether `ZrynaValue` is the only
initial bound and whether explicit application syntax can be represented without
ambiguity by the replacement frontend. A local parser check using installed
`@typescript/typescript6` 6.0.2 accepts the member-call and direct-call
candidates above and rejects `Option<i32>.some(7)`; the adapter's expected 6.0.3,
provider-neutral and native-provider evidence remains uncollected. The candidate
limits, diagnostics and canonical layout encodings
require exact-limit/first-extra and independent digest fixtures before adoption.
The public ABI of these types, aggregate export policy, WIT/component mapping and
host resource policy remain separate proposals. No current supported profile is
expanded by approving internal semantics.

Issue #415 acceptance remains open until the reviewer approves the candidate
bound, syntax, exact diagnostic codes, instance budgets and successor encoding;
schema/fixed-fixture/ambiguity/dependency checks cover that approved design; and
the missing generic nominal/Result record fixtures and independent full digest
checks are supplied. This draft does not waive those issue requirements or claim
that the public ABI or #400 host decisions have been settled.

Dependency-ready slices are: (1) syntax protocol and provider-neutral fixtures;
(2) semantic closed-type and deterministic instantiation authority; (3) versioned
layout and verified IR extensions with hostile fixtures; (4) owned Option/Result
construction, matching and cleanup; (5) three backend/runtime conformance; and
(6) driver/public activation after exact-revision gates. Each slice retains the
existing M3 ownership and #357 profile-composition prerequisites and may not
substitute a backend or host permission for them.
