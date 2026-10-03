# Bounded generics, Option, and Result v1

Contract identity: `zryna.bounded-generics-option-result.v1`. State:
**specified-only; no public or compiler activation**. The syntax, diagnostic
codes, binary keys and limits below define the reviewed future internal contract.
On normal PR integration, the complete companion packet is the Issue #415
specification. Integration does not change `DataOwnershipV1`, syntax
protocol v4, verified IR, aggregate layout v1, scalar ABI v1, any target backend, or
the public profile. Issue #416 must implement the exact encoding and independently
verify it before executable use; its separately versioned protocol remains a gate.

Companion contracts cover the [verified IR](../ir/GENERIC_INSTANTIATION_V1.md),
[closed layout](../memory-model/GENERIC_ENUM_LAYOUT_V1.md),
[ABI boundary](../abi/GENERIC_VALUE_BOUNDARIES_V1.md), and
[conformance/resource evidence](../../docs/M7_GENERIC_CONFORMANCE.md).
The [complete decision catalogue](BOUNDED_GENERICS_DECISIONS_V1.md) fixes exact
source forms, exclusions, Copy/Clone, borrowed match, diagnostic selection and
the dependency-ready #416 plan. Read these documents as one contract unit.

## 1. Authority and admitted forms

This future contract extends the [M3 ownership contract](DATA_OWNERSHIP_V1.md) without
reinterpreting existing values. Names resolve through the authenticated final M2
module map. Source type checking, instantiation and ownership checking belong to
Zryna semantics; a provider reports source syntax and spans only. The mandatory IR
verifier independently checks closed types, call substitutions and cleanup plans.

The initial user-defined forms are top-level functions and nominal struct/enum
declarations with one or two invariant type parameters. A parameter occurs only in
value parameter/result types, fields, variant payloads, and admitted nested M3
containers. No locally declared generic functions or types, methods, closures, aliases, interfaces
other than the nominal markers, or executable generic exports at scalar ABI v1 are admitted.
Source-module `export` on a template exposes only its compile-time declaration
for exact named imports; it never creates an executable export or host symbol.
All function parameters and results retain explicit types. Generic values must be
fully instantiated before layout, IR sealing or code generation. Bare generic names
are never runtime values.

The specified TypeScript-compatible source spelling is:

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
object type, an implicit conversion or an erasable runtime trait. Its specified
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
excluded. Importing a generic function or data declaration follows the existing
exact named-import and module-closure rules. Its original declaration identity
survives every import path and diamond. A source-level generic function name remains unique in
its module: overload sets and specialization are excluded. The source call graph,
including imported generic calls, remains acyclic. Instantiation cannot introduce
direct or mutual function recursion. A future syntax protocol must define distinct nodes
for parameter declarations, bound spans, closed type applications and explicit call
arguments; protocol v4 must reject these forms unchanged.

## 2. Closed instantiation and monomorphization

Closed data instances use the exact specified type keys in the
[successor layout contract](../memory-model/GENERIC_ENUM_LAYOUT_V1.md): tags
`12`/`13` plus `(ModuleId, data-declaration-index)` for user generic structs
and enums, and reserved tags `14`/`15` plus arguments for compiler-owned
`Option`/`Result`. The latter have no source module or declaration index.
Function keys use a disjoint specified namespace:

```text
40 || u32 ModuleId || u32 functionIndex || u32 argCount || childKey[argCount]
   closed generic function instance
41 || u32 ModuleId || u32 functionIndex
   non-generic source function root
childKey = u32 byteLength || complete canonical type key bytes
```

All integer lanes are unsigned little-endian. `functionIndex` is zero-based
source order among functions in the authenticated module, independent of the
data-declaration index. The `40` count is one or two; a `41` key has no count
or arguments and is never a monomorphized instance. The first byte separates
functions from every admitted type key. For ModuleId 0, functionIndex 0,
`identity<i32>` has key `400000000000000000010000000100000001` and SHA-256
`239cb3a761f38e6229ed98a8bfdc2325c4819bd85a0a1b45b8416a324dd70b05`;
the nongeneric root has key `410000000000000000`. A mixed unsigned bytewise
fixture orders `Box<i32>` (tag `12`), `Option<i32>` (tag `14`),
`identity<i32>` (tag `40`), then the root (tag `41`). The root participates
in edge identity but not the pending generic-instance inventory. Source
spelling, alias path, traversal order, target and host address never enter a
key. Distinct instances with identical bodies do not merge.

Discovery starts from all admitted non-generic function roots and fully closed
data types in the authenticated module graph. Walk declarations in ascending
ModuleId, kind (`data` before `function`) and source index; within each body
visit type annotations, expressions and calls in source order. Enqueue each new
closed generic function or data instance by its canonical key, then process the
smallest pending key by unsigned bytewise order. Each instance body is substituted
once, checked, and added to a globally deduplicated inventory. The final generic
function inventory is sorted by tag-`40` key and assigned dense function-instance
IDs. The successor layout authority separately assigns TypeIds from its complete
type universe; nongeneric function roots retain existing function identity and
receive no monomorphization ID. A later backend may choose private symbol
spellings only from these sealed IDs and verified keys; it may not discover new
instances. The same closed source graph gives the same IDs across targets and
repeated builds.

Specified compile-time ceilings are below. Every count uses checked arithmetic;
the exact-limit member is admitted and the first extra member is rejected before
producing a partial inventory or IR. Existing stricter source, type, layout, IR and
runtime limits still apply.

| Per authenticated compilation | Maximum |
| --- | ---: |
| Type parameters on one declaration | 2 |
| Explicit type arguments at one use | 2 |
| Distinct closed generic function instances | 4,096 |
| Distinct closed generic data instances (user nominal plus Option/Result) | 4,096 |
| Total closed instantiation dependency edges | 65,536 |
| Maximum nested closed type application depth | 64 |
| Maximum key bytes for one closed instance | 4,096 |

The depth limit counts application nodes in a complete canonical type key:
`bool`, `i32`, `String` and a non-generic nominal key have depth zero;
`[T; N]`, `Vec<T>`, `Shared<T>`, `Weak<T>`, a closed generic nominal,
`Option<T>` and `Result<T,E>` each have depth one plus the maximum depth
of their type arguments (or array element). A generic function key has no
type-application level of its own; each supplied type argument is checked
independently. Nominal fields and variant payloads do not unfold inside a
type key. Thus 64 nested `Option` applications around `i32` are admitted
by this ceiling and 65 are the first-extra `ZRYNA-M7201` rejection, subject
to the other independent ceilings. An outer `Vec` consumes one level:
`Vec<Option^63<i32>>` reaches 64 and `Vec<Option^64<i32>>` reaches 65.
The fixed instantiation fixtures use `Option^N<i32>` as shorthand for N
nested applications, not source syntax, and pin these keys and outcomes.

An instantiation dependency edge is one distinct ordered pair of keys
`(from, to)`. `from` is a generic instance, a nongeneric function root (`41`),
or a nongeneric nominal type key (`10`/`11`) whose body is being checked.
`to` is a closed generic function (`40`) or closed generic data type (`12`–`15`)
referenced by its substituted body, including through M3 container wrappers.
For a generic function, calls and type occurrences produce edges. For a data
declaration, field and variant type occurrences produce edges. Repeated
occurrences of the same pair count once. Scalar and nongeneric targets do not
count. A self-edge counts once. Roots count in the edge inventory but neither
the generic function nor generic data 4,096-instance ceiling.
The edge inventory is sorted by `(from key, to key)` before assigning IDs or
checking the limit. A synthetic fixture with 65,536 distinct pairs is accepted;
adding the lexicographically next pair is the first-extra rejection, even when
the same pair also occurs multiple times in source. Synthetic graph fixtures
exercise this budget independently of stricter source-size limits.

These ceilings do not increase M3's 65,536 fully instantiated type budget or
256-diagnostic ceiling. A bounded work queue and explicit stack implement discovery;
host call-stack depth and map insertion order cannot affect acceptance. Function
recursion is rejected on the source-level call graph, regardless of equal or
different type arguments. Supplied explicit type-argument trees are finite and
are validated before declaration-body expansion. An occurrence inherited from
a parameter substitution, including a nested subtree of a supplied argument,
is a finite supplied instance and does not trigger the expanding-declaration
rule. Thus `Box<Box<i32>>` with `Box<T> { value: T }` admits both closed Box
instances: the inner Box is already in the supplied argument tree. For data
type expansion, a repeated *same closed key* is deduplicated and terminates
traversal; its by-value cycle is separately rejected by layout, while an
indirection cycle such as `Node<T> { next: Vec<Node<T>> }` remains legal.
Only a nominal application head introduced by a declaration's own field/variant
type expression (rather than copied from a supplied argument subtree) triggers
the expanding-declaration check. If that generated head repeats its generic
nominal declaration with a different argument key on the generated-expansion
path, it is rejected before further expansion. For example,
`Nest<T> { next: Vec<Nest<Vec<T>>> }` is rejected, including when entered
through a finite outer argument; `Nest<T> { next: Vec<Nest<T>> }` has the same
key and is legal by indirection. The diagnostic identifies the lexicographically
smallest offending canonical path. The
[fixed instantiation fixtures](generic-instantiation-v1-fixtures.json) name the
finite supplied-nesting, same-key indirection and expanding-generated cases.
This deliberate bounded rule may reject finite alternating generated instances;
later widening requires a new contract.

## 3. Option and Result

`Option<T>` and `Result<T, E>` are reserved, compiler-owned nominal enum families,
not user-definable or structurally compatible with a same-shaped enum. Their only
admitted arguments satisfy `ZrynaValue`. Constructor and match notation is a
specified extension of the v4 enum forms for the future protocol:

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
internal representation. In particular this contract makes no #400 F1/F2 host
permission decision. Existing `upgradeWeak` keeps its two-successor operation;
it does not silently start constructing `Option<Shared<T>>`.

## 4. Failure, diagnostics and conformance gates

Malformed source, unknown/non-value arguments, missing or extra arguments, illegal
generic operations, unclosed types, forbidden recursion, invalid nominal identity,
wrong variant and nonexhaustive match are compile-time errors before backend
emission. These exact codes belong to the future versioned language/IR contract;
no current executable path may emit them:

| Specified code | Owning rejection |
| --- | --- |
| `ZRYNA-D7001` | Invalid generic declaration form, bound or parameter list |
| `ZRYNA-M7001` | Missing, extra, unclosed or bound-violating type argument |
| `ZRYNA-M7002` | Operation unavailable for opaque bounded parameter |
| `ZRYNA-M7003` | Function recursion or expanding nominal instantiation |
| `ZRYNA-M7004` | Wrong standard variant or inexact/nonexhaustive match |
| `ZRYNA-M7005` | Generic or standard enum at a forbidden public ABI boundary |
| `ZRYNA-M7006` | Exact payload, field, call or match-result type/arity mismatch |
| `ZRYNA-M7007` | Generic/standard-enum move, borrow, initialization or cleanup misuse |
| `ZRYNA-M7201` | Terminal instantiation resource exhaustion |
| `ZRYNA-I7001` | Unclosed, mismatched or forged verified-IR instance/variant/cleanup |
| `ZRYNA-L7001` | Invalid closed layout key, record, ordinal or fingerprint |
| `ZRYNA-L7201` | Terminal closed-layout resource exhaustion |

Malformed provider syntax must fail in a future versioned syntax verifier before
semantic codes apply; it cannot be retrofitted to v4. Selection follows
authoritative source location, then canonical instance key and complete numeric
tie-break data. Budget exhaustion is terminal and cannot return a partial sealed
program. No existing diagnostic code is reassigned by this contract.

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

The specified decision is exactly one initial bound, `ZrynaValue`, and explicit
application syntax as fixed in the complete decision catalogue. Checked-in
review fixtures and pinned TypeScript 6.0.3 AST checks make syntax ambiguity
review reproducible. They do not implement a future provider-neutral protocol;
its schema and native-provider conformance remain #416 work. Specified limits,
diagnostics and canonical layout encodings are reviewed with the fixed schema,
exact-limit/first-extra reference fixtures and independent digest checks.
The public ABI of these types, aggregate export policy, WIT/component mapping and
host resource policy remain separate proposals. No current supported profile is
expanded by approving internal semantics.

The bound, syntax, exact diagnostic codes, instance budgets, successor encoding,
and fixture/schema/ambiguity/dependency packet are fixed by this engineering
review. Required repository checks and normal PR integration still govern
specification delivery. After integration the contract state is specified-only;
provider, compiler, runtime, backend and public activation evidence remains #416
implementation work. Neither contract review nor integration establishes a
public aggregate ABI or settles #400 host decisions.

Dependency-ready slices are: (1) syntax protocol and provider-neutral fixtures;
(2) semantic closed-type and deterministic instantiation authority; (3) versioned
layout and verified IR extensions with hostile fixtures; (4) owned Option/Result
construction, matching and cleanup; (5) three backend/runtime conformance; and
(6) driver/public activation after exact-revision gates. Each slice retains the
existing M3 ownership and #357 profile-composition prerequisites and may not
substitute a backend or host permission for them.
