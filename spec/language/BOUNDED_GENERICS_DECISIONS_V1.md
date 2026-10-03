# Bounded generic decisions and acceptance v1

State: **specified-only**. This is the normative completion of the
[language contract](BOUNDED_GENERICS_OPTION_RESULT_V1.md) for Issue #415.
The fixture schema below is a specification-review format, not a syntax protocol,
IR wire format, CLI selector or executable compiler. Normal PR integration
records the complete reviewed packet as the specified-only future contract;
implementation and public admission require separate exact-revision evidence.

## Exact source forms and exclusions

Identifiers, literal spellings, imports, bodies, statement forms, places and
container restrictions inherit M3 and [syntax v4](../../docs/SYNTAX_PROTOCOL_V4.md).
Only the following generic extensions are admitted. Whitespace and comments
retain the existing token rules; names resolve through the authenticated graph.
`T` and `E` below denote unique ASCII parameter identifiers, not fixed names.

| Form | Exact spelling and decision | Invalid form / diagnostic |
| --- | --- | --- |
| Function template | `function f<T extends ZrynaValue>(x: T): T { return x; }`; two parameters use `<T extends ZrynaValue, E extends ZrynaValue>` | Empty/duplicate/more than two parameters, missing bound or result annotation: D7001 |
| Struct template | `interface Box<T extends ZrynaValue> extends ZrynaStruct { value: T; }` | Another heritage marker, empty fields, optional/readonly/computed property, method, merging: D7001 |
| Enum template | `interface Choice<T extends ZrynaValue, E extends ZrynaValue> extends ZrynaEnum { ok: T; err: E; none: ZrynaNone; }` | Invalid/duplicate/empty variants, explicit ordinal: D7001 |
| Declaration export | `export interface Box<...>` and `export function f<...>` expose compile-time declarations to named imports; templates emit no executable export | Requesting a template or specialization as scalar ABI entrypoint: M7005 |
| Closed annotation | `Box<i32>`, `Option<String>`, `Result<i32, String>`, nested `Vec<Option<i32>>`, `FixedArray<Option<i32>, 2>` | Bare generic name, missing/extra/unknown/borrow/unit argument: M7001 |
| Function use | `f<i32>(7)`; exact explicit argument list required at every call | Inference, partial application, template-as-value: M7001 |
| Struct construction | `Box<i32>({ value: 7 })`; declaration-order evaluation and exact fields | Wrong/duplicate/missing field or wrong value type: M7006 |
| User enum construction | `Choice.ok<i32, String>(7)`, `Choice.err<i32, String>(message)`, `Choice.none<i32, String>()` | Wrong variant: M7004; wrong payload count/type: M7006 |
| Standard construction | `Option.none<T>()`, `Option.some<T>(value)`, `Result.ok<T,E>(value)`, `Result.err<T,E>(value)` | Same variant/arity/type decisions as user enum |
| By-value match | `match(value, { "Option.none": () => 0, "Option.some": (item) => item })` | Missing/duplicate/wrong family/unknown arm, wildcard, guards, block arrow, wrong binding count: M7004 |
| Borrowed match | `const loan: Borrow<Option<String>> = borrow(owner); match(loan, { "Option.none": () => 0, "Option.some": (item) => 1 })` | Binding move, owner mutation/drop or escaping loan: M7007 |
| Module use | `import { Box, identity } from "./values.zry"; identity<i32>(7)`; source-visible template and data declarations use exact import rules | Wrong identity/arity uses M7001; malformed graph/import retains existing module diagnostics |

Numeric diagnostic suffixes in this table mean `ZRYNA-<suffix>`. Generic
parameter scope is the complete owning declaration including its body, never
another declaration. Parameters must not shadow a reserved type/marker or an
authenticated visible type name; duplicate/shadowing declaration names use
D7001. `Option`, `Result`, `ZrynaValue`, `ZrynaStruct`, `ZrynaEnum` and
`ZrynaNone` cannot be redefined or imported under those reserved names.
There is no declaration merging or overload resolution. A normal nongeneric
scalar export may call a local or imported generic function with a closed signature.
In this future contract, `export` on a generic function means source-module
template visibility only. A template has no executable address or public ABI
signature. Its closed instances remain internal even when every substituted
type is scalar. A separately verified executable-export inventory contains
only nongeneric scalar functions. Selecting a template/instance as a public
entrypoint or forging it into that inventory uses M7005/I7001 respectively.
This is explicit new-version behavior, not a reinterpretation of current v4
exports. Generic templates are checked even when unused; only their demanded
closed instances consume the instance budget.

The following are excluded in every context: local generic declarations,
classes, methods, closures other than the exact match arrows, type aliases,
structural object types, unions/intersections, conditional/mapped/indexed types,
`keyof`, `typeof` type queries, literal types, `infer`, constraints other than
the exact single bound, default parameters, variance annotations, higher-kinded
parameters, specialization, overloads, variadic type parameters, rest/spread
arguments, generic function values, implicit dispatch/erasure, exceptions and
implicit propagation. Declaration/type-syntax violations use D7001; use of a
template without complete arguments uses M7001; operations relying on a stronger
bound use M7002. No layout, ownership or ABI exists for a rejected form.
`Option<i32>.some(7)` is a parser rejection; `Option.some(7)` is valid TypeScript
syntax but M7001. Neither form is interpreted as another constructor.

The fixed source catalogue supplies exact spellings for every representable
excluded category, including `class Box<T ...>`, `get(): T`, `value?: T`,
`readonly value: T`, `[name]: T`, duplicate declarations, overload signatures,
`T | i32`, `T & i32`, `T extends i32 ? i32 : bool`, mapped/indexed types,
`keyof`, `typeof`, literal/inferred types, multiple bounds, `out T`, rest/spread,
generic arrow values, wildcard/block match arms, template values, and `throw`.
Higher-kinded use `F<i32>` where F is an opaque value parameter is M7002;
specialization cannot declare a second body for the same name (D7001).
Dynamic dispatch, erased/untyped carriers and implicit error propagation have
no admitted syntax or representation and cannot be synthesized by a provider.
Its primary-token occurrence pins the intended half-open UTF-8 source range,
including the second duplicate arm and consuming use rather than a binding.
These remain future diagnostic fixtures, not current compiler output.

`Borrow<T>` and `BorrowMut<T>` may occur in a template's ordinary private
signature or local borrow annotation wherever M3 permits a borrow, but neither
can be a `ZrynaValue` argument or a stored field/payload. Borrow creation and
nonescape retain M3 regions. No borrow-bearing type key enters the layout universe.
A generic body may construct/store/move/borrow/pass/return opaque `T`; it may
not clone, compare, perform arithmetic, project, index or call a type-specific
operation on `T`. It can call another exact bounded template with `T` before
closed substitution; the executable instance must contain complete keys.

## Copy, clone, match and failure decisions

`Option<T>` is Copy iff T is Copy; `Result<T,E>` iff both arguments are Copy.
User nominal instances derive Copy from every substituted stored field/payload,
including every possible variant. The inactive variant does not make
`Option<String>.none` Copy. Structural Clone uses the corresponding all-payload
rule; `clone(Option<String>)` clones only the active String. `Shared`/`Weak`
payloads retain checked handle clone rather than payload clone. Opaque T has
neither capability available to its template body. Derivation is a finite graph
calculation under M3 rules, never an implicit clone or a new trait system.

A by-value match consumes an owned scrutinee; a Copy scrutinee is read without
invalidating its source. A payload-free arm takes zero bindings; a payload arm
takes exactly one. Its result has the exact common declared result type, without
coercion; mismatched arm results use M7006. Keys name the syntactically visible
family, with no `<...>` text inside the string. Resolution must prove every arm
names the scrutinee's same closed nominal identity. Source arm order cannot
alter fixed variant ordinals. No wildcard or fallthrough is admitted.

For a shared-loan scrutinee the payload binding is `Borrow<Payload>`; it may
produce permitted Copy observations or an explicit permitted clone, and it
cannot move the payload. An exclusive-loan scrutinee binds
`BorrowMut<Payload>` and permits only the inherited exact M3 mutation operations.
Each binding ends before the selected arm exits. No binding survives to a join
or return, and the scrutinee loan remains live until its original lexical end.
Borrowing a temporary scrutinee or constructing a borrow from the match result
is excluded. These are future admission obligations, not current M3 checkpoints.

Construction of a standard enum adds no allocation by itself. Its one payload
is evaluated before the enum becomes initialized; a failing payload leaves no
initialized enum or invented active-payload owner. User struct initializers run
in field declaration order, and failure drops only the initialized field prefix
in reverse before earlier live roots. For owned match, ownership of the active
payload transfers once to the binding; the consumed empty enum adds no second
payload drop. On successful arm exit drop remaining arm locals in reverse, then
the bound payload if still live. A returned owner is excluded from local cleanup.
On controlled failure unwind the same live-owner plan, then enclosing roots in
reverse. A borrowed match ends arm loans before enclosing owner cleanup.
Inactive bytes are never initialized, observed or released.

The five exact inherited traps are `zryna.trap.bounds-v1`,
`zryna.trap.allocation-v1`, `zryna.trap.capacity-v1`,
`zryna.trap.refcount-v1`, and `zryna.trap.utf8-v1`. `Result.err` is a successful
value observation; `none` is not allocation failure. No exception, host error,
engine trap, signal or ABI violation is converted into an enum value. Release
must not allocate or fail as a source trap. External termination, engine failure
and memory corruption have no controlled-cleanup promise, as in runtime ABI v1.

## Diagnostic selection and resource interpretation

The specified codes are M7006 for exact value/field/call/arm type or payload-arity
mismatch, and M7007 for move, borrow, initialization or cleanup misuse introduced
by generic/standard-enum operations. By-value closed-type cycles retain L3002;
checked object-size/alignment arithmetic retains L3005. Existing errors on an
unchanged nongeneric program retain their existing codes and bytes. Malformed
raw future syntax rejects at the syntax authority before semantics; its wire
code allocation belongs to the future protocol and does not repurpose v4.

Primary spans are half-open UTF-8 bytes from the exact SourceMap: D7001 uses the
offending declaration token/bound; M7001 the explicit argument (or callee name
when omitted); M7002 the operation name; M7003 the closing call or generated
application head; M7004 the offending arm key (missing coverage uses `match`);
M7005 the exported function name; M7006 the offending value/field (payload arity
uses the constructor/call name); M7007 the invalid consuming/borrow operation.
Synthetic resource/IR/layout cases have global locations and no invented span.
Resource diagnostics include metric, limit, first-extra count and canonical
offending key when authenticated; no partially sealed result is returned.

Phase order is raw syntax validation, declaration shape, module/name resolution,
opaque-body checking, closed argument validation/discovery, ownership, layout,
IR sealing, ABI verification, backend emission. ABI signature preflight may
reject before emission but cannot make invalid earlier syntax valid. Within a
phase sort diagnostics by `(FileId, start, end, code ASCII bytes, instance key
unsigned bytes, numeric witness sequence)`; missing/global source locations
sort after source locations. Equal diagnostics deduplicate by this whole tuple.
The 256 diagnostic slots include a reserved terminal diagnostic: retain at most
255 ordinary entries, and the 256th candidate terminates with the owning budget
diagnostic. Semantic terminal code is M7201; layout is L7201; IR retains its
inherited budget code. No further phase runs after terminal failure.

The one/two-argument limits are grammar/arity decisions: a third declaration
parameter is D7001 and a third use argument M7001, before resource counting.
The five remaining instantiation ceilings use M7201 on the first extra, with
the inherited stricter authority applied independently. Duplicate keys/edges
do not consume additional inventory; traversal must stop before expansion of
an extra member. Checked-add overflow rejects without wrapping. Synthetic
exact/first-extra graphs/byte arrays test limits independently of source budgets;
they do not claim those synthetic inputs are source-admissible programs.

## Compatibility, dependencies and delivery

This specification is one future universal internal language contract under #357;
it is not a subtype/relabeling of existing DataOwnershipV1 authorities. A pure
generic dependency chain has an explicit verified empty capability set. A
transitive host operation still fails #357 profile validation before IR;
unused templates do not remove dependency requirements. Host grants, package
instance IDs, WIT and native FFI remain separate authorities. No selector,
manifest field, exported aggregate carrier or host effect is added here.
Standard family keys have no source module; user instance identity is scoped to
the final authenticated graph and cannot be reused across package compilations
without a separately accepted linking/instance-identity contract.

The review packet is the [conformance map](../../docs/M7_GENERIC_CONFORMANCE.md),
the closed [review fixture schema](generic-review-v1.schema.json),
[fixed review cases](generic-review-v1-fixtures.json), existing instance/layout
byte fixtures and their independent tests. Schema validation and TypeScript
AST ambiguity checks prove the specification packet's shape/spelling only.
Recorded semantic observations and cleanup traces are frozen requirements for
#416; they are not executed Zryna results.

| #416 slice | Prerequisite and owner | Measurable exit |
| --- | --- | --- |
| Syntax | Normally integrated reviewed #415 revision; syntax foundation then bootstrap/native providers | New separately versioned closed schema, source-faithful DTOs, exact spans, malformed DTO rejection, all catalogue forms; v2/v3/v4 bytes/diagnostics unchanged |
| Instantiation | Verified syntax and final module graph; semantics | Opaque bodies, deterministic complete keys/IDs/edges, recursion decisions, cross-module dedup, every exact/extra ceiling with no partial result |
| Layout/IR | Closed inventory; layout, IR and ownership-runtime ABI authorities | Successor sealed layouts and hostile raw IR/records, owned/nested digests on both storage targets, independent failure/cleanup verification |
| Owned operations | Verified instance/layout authorities; semantics and ownership IR | All four standard constructors, exact value/shared/exclusive match, Copy/Clone derivation, moves/returns and all controlled fault traces |
| Targets | Same sealed program; independent JS, core Wasm and admitted native backends/runtimes | Fixed scalar observations and logical drop/release traces; every trap ordinal, deterministic artifact replay, supported Linux/Windows host gates |
| Driver admission | All preceding exact-revision proofs; driver/profile authority | Explicit separately reviewed selection/versioning, authenticated manifest/interface and compatibility, no grants/export widening |

No implementation slice starts before the reviewed #415 specification is
normally integrated. Its specified-only state establishes the contract and
reference evidence, not any #416 execution or public support. Every unchanged
existing profile retains byte/diagnostic compatibility, including `upgradeWeak`.
