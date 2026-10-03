# Bounded generic syntax protocol v5 candidate

This internal candidate for [Issue #416](https://github.com/zryna/zryna/issues/416)
contains an untrusted raw grammar, source-backed declaration checks and a separately
authored complete-source/arena verifier for the
[accepted bounded generic contract](../spec/language/BOUNDED_GENERICS_OPTION_RESULT_V1.md).
The success-verifier successor is under source review and has not been compiled or
exercised in Rust. It has no provider registration,
semantic admission, public selector, layout, IR, runtime or target capability. The enclosing
issue remains open.
Protocol v2/v3/v4 and their routes remain separate and unchanged.

The independent [JSON schema](../schemas/zryna-syntax-v5.schema.json) closes the complete
raw DTO grammar. `zryna_syntax::v5::decode_snapshot` returns only raw claims. The new
declaration check returns `Result<(), DeclarationError>`; no successfully checked
declaration inventory can be passed to current semantics or interpreted as v4 authority.
Full body/arena/source verification and two-provider differential conformance remain
required before a future opaque executable snapshot exists.

## Wire records

The envelope is exactly `{ schema_version: 5, files, diagnostics }`. Source units retain
`{ id, path, imports, type_syntax, data_declarations, functions }` and dense SourceMap
file order. Each object rejects unknown and omitted fields, including nullable fields.
Duplicate JSON keys, a second value after the response and another protocol version
reject. Decoder response bytes and inherited per-record inventories retain v4 ceilings;
declaration validation independently preflights source and project-level inventories.
These limits do not implement monomorphization, closed-key or layout budgets.

Every data/function declaration adds the required nullable `type_parameters` field.
Null means a nongeneric declaration; a list contains exactly one or two parameters:

```text
TypeParameterList { span, less_than_span, parameters, comma_spans, greater_than_span }
TypeParameter { span, name, extends_span, bound }
TypeArgumentList { span, less_than_span, arguments, comma_spans, greater_than_span }
```

Parameters and bounds are separate source-spelled identifiers and exact UTF-8 spans.
`bound` preserves syntax; declaration validation checks the exact `ZrynaValue` marker,
unique names and local visible type/reserved-name shadowing. It never assigns trait,
Copy/Clone, type identity or ordinal. Imported type shadowing requires final module/name
resolution and is deferred along with opaque-body checking.

The module type arena adds `{ kind: "application", name, type_arguments }` for named
generic applications, including Option and Result. Argument IDs name distinct source
type occurrences in the same arena. Existing named/container/borrow/fixed-array records
remain syntax forms; an opaque parameter occurrence is a named identifier whose scope
is resolved later. Every type child precedes its parent. Containers retain their own
syntax tags; they cannot be substituted with a named application record. Syntax depth
retains 128, independently of the later closed-type depth-64 ceiling.

Call, struct-construction and enum-construction kinds add required nullable
`type_arguments`. Null preserves an omitted list for later M7001 validation and does
not request inference. Lists retain each explicit type occurrence, delimiters and
source-ordered comma tokens (including an optional trailing comma). A nonnull list
has one or two arguments. Other expression/statement tags retain their v4 DTO shapes;
all of them remain untrusted in this slice.

For `Option.some<i32>(7)` and `Choice.ok<i32, String>(7)`, the list follows the member
identifier and precedes `(`. A call or construction never records a resolved instance,
family ordinal, layout, inferred type, ownership transfer or backend symbol. There is
no node for `Option<i32>.some(7)`, specialization or a template runtime value.
Match arms retain the visible family/variant spellings without type arguments in the
key; exhaustive identity, payload bindings and borrow mode belong to later semantics.

`export` retains its exact source span. A generic template's compile-time module
visibility does not create an executable export inventory or public ABI signature.
Only the eventual separate semantic/IR/ABI authorities can implement that distinction.

## Declaration checks and failure boundaries

`validate_declarations` binds the complete file set, paths and header/data/type spans
to one supplied SourceMap. It consumes all top-level significant input in source order,
including imports and interleaved data/functions. Missing, reordered or extra declarations,
hidden top-level input, source-inexact tokens, foreign files, invalid UTF-8 boundaries,
missing explicit annotations and malformed type edges reject. Data bodies require every
field/variant and exact punctuation; function signatures require explicit parameter and
result types. Type and declaration outer spans have canonical token boundaries.

`validate_declarations` checks function bodies only as complete balanced braced ranges. Their internal
statements, expressions, references, application lists, type roots, arena ownership and
graph order are **not authenticated**. Body grammar, local generics, stronger operations
on opaque parameters and all ownership/cleanup checks remain later gates. Likewise,
declaration validation does not resolve imports, prove final module closure, instantiate,
check bounds on closed arguments or produce executable authority. A hostile body arena
may therefore pass declaration validation; a dedicated test documents this boundary.

The new syntax-only diagnostic allocation is `ZRYNA-Y5001` for malformed v5 wire/source
claims and `ZRYNA-Y5201` for terminal v5 response/source inventory exhaustion. Existing
diagnostic namespaces are unchanged. `ZRYNA-D7001` is the accepted declaration diagnostic
for a source-authenticated invalid bound, duplicate/reserved declaration/parameter name
or invalid declaration shape. Malformed provider claims fail before a declaration error
can trust their spelling. Invalid or synthetic ranges have no invented source location.
The declaration-only check stops at its first failure and returns no partial authority.
It remains a separate API; passing it cannot replace complete source/arena verification.

## Success-verifier successor under review

`verify_snapshot(raw, sources)` is the proposed normal-success entry point. Its source
checks consume complete imports, headers, data bodies, blocks and statements; every
expression and type occurrence must have one owner and exact canonical postorder.
Block/statement arrays must equal source preorder, with a zero root and bounded iterative
traversal. Complete significant-source coverage authenticates punctuation missing from
wire leaf records, quoted match keys/arrows, weak-upgrade statements and shorthand aliases.
Operator shape checks enforce precedence and left associativity, rather than trusting a
provider tree with faithful individual token strings. Grouping parentheses remain excluded.

Every file's raw/source/arena checks must finish before declaration admission can return
D7001. Malformed claims elsewhere in the project therefore precede a genuine declaration
error. Syntax failures are sorted/deduplicated by authenticated source location and code,
with global locations last and a reserved terminal diagnostic slot. No instance identity
or semantic witness is assigned in this syntax phase.

The only constructor of `VerifiedProjectSyntaxV5` requires a private complete-syntax proof.
The value owns immutable syntax and the exact `SourceMap` identity. Read-only files retain
unresolved names/type arguments; provider diagnostic text remains advisory. There is no
v4 conversion, deserialization constructor, semantic/IR seal or public profile activation.
Identifier roles distinguish bindings, references/callees, member/quoted labels and
type heads. Authenticated import/export prefixes establish per-file module context;
complete declaration inventory and EOF coverage prevent hidden prefixes from downgrading
that context. Function-owned type occurrences inherit ordinary-function context through
the complete type forest. Strict module bindings and bare assignments reject `eval` and
`arguments`, while read-only calls/references and member labels retain their own rules.
Contextual names such as `from`, `as` and function-owned `await` remain permitted where
the frozen TypeScript source form admits them. Primitive/operator type AST spellings
cannot be relabeled as nominal type heads. Strict directive prologues are excluded from
this grammar. These checks emit Y5001; declaration and semantic name rules remain separate.
Bare `function` is an authentic Named type-reference spelling in the pinned parser;
`function<i32>` does not parse as a complete type application and rejects an Application
claim. The source check preserves this distinction without resolving the named type.
Return values require no intervening LF, CR, U+2028 or U+2029, including inside comments;
otherwise automatic semicolon insertion separates the return from the following expression.
Line comments end at each of those four terminators so significant input after a Unicode
separator cannot disappear from the claimed declaration inventory.

The successful DTO continues to admit one/two generic parameters/arguments and at most one
enum payload expression or match binding. Excluded forms requiring longer argument or
parameter lists, block match arrows or missing annotations need a separately reviewed,
complete diagnostic-only source context and owning-phase handoff. No failure transport is
implemented here, and syntax cannot select M7001/M7004/M7006 from a provider form tag.

## Evidence and remaining work

The independently authored [two-module wire reference](../tests/m7-syntax-fixtures/reference.json)
and its [source](../tests/m7-syntax-fixtures/main.zry) preserve imported templates, one/two
parameters, i32/String applications, nominal construction, all four standard constructors,
exact match spelling and a UTF-8 comment prefix. Fixed digests bind the new references and
unchanged v2/v3/v4 source/schema bytes. They are raw syntax examples, not executed target results.

`node --test tests/syntax-protocol-v5.test.mjs` checks closed records, every populated
required field, raw tag inventories, exact/first-extra parameter/argument counts, independent
source spans, member/list placement and old-protocol rejection. `cargo test --locked -p
zryna-syntax v5::` is the Rust declaration/hostile-decoder suite. The cloud continuation
executes the focused suite; its exact-revision receipt records command and test counts.

The independent precedence, control-flow and ownership-operation source/raw fixtures cover
all 28 expression and eight statement tags. They assert syntax only, including source whose
semantics must reject later. The new Rust tests cover omitted source despite clean owned
arenas, faithful-token precedence/associativity forgeries, cross-file raw-before-declaration
barriers, match/weak delimiters, source identity, hostile endpoints and diagnostic limits.
The cloud continuation executes these successor tests. Earlier declaration-only results
remain evidence for their own revision; the continuation receipt identifies the current source.

Independent script/module keyword fixtures cover permitted contextual bindings, read-only
strict names, keyword member/quoted labels, an aliased keyword import and function-owned
type names. The strict bare-assignment negative fixture has a closed schema and clean
occurrence forests, and parses without errors; parser acceptance alone does not prove
its strict identifier role. Source tests cover keyword relabeling, type ownership, hidden
export prefixes, shorthand aliases, weak/match bindings and strict directive rejection.

Complete successor review, both providers and broader excluded-source conformance remain
the syntax slice's next steps. Closed semantic discovery is an internal candidate in the
[continuation](M7_GENERIC_CONTINUATION.md); complete instantiation conformance, successor sealed layout/IR,
real owned Option/Result construction/matching/cleanup, hostile authorities, cross-target
fault/conformance and separately reviewed driver admission remain later #416 work.
Required preflight/M0/platform gates are not waived by these focused checks.
