# Zryna frontend contract

Versioned, provider-neutral boundary for replaceable TypeScript and native Zryna frontends.

Provider output is untrusted. Protocol-v1 adapters retain their declaration-only legacy contract.

`native_parser::v3::discover_import_candidates` extracts only untrusted top-level import candidates
from original bound native tokens. It skips balanced bodies without constructing executable syntax.
The driver alone resolves these candidates through retained source capabilities, seals one original
source map and graph, then requires complete parsing and existing versioned syntax verification.
See [native source snapshots](../../docs/NATIVE_SOURCE_SNAPSHOTS.md) for ownership and exclusions.
The protocol-v2 and protocol-v3 process runners launch an absolute executable directly without a
shell, perform an exact identity/version/protocol/capability handshake, and only then send the
authoritative `SourceMap` contents for analysis. Their typed expectations and verified result APIs
are separate, so neither transport can reinterpret or silently upgrade the other. After the
operating system returns a successful spawn, one
monotonic deadline covers handshake, analysis, pipe drains, process exit, and reserved cleanup.
The worker starts in a fresh Unix process group or Windows Job Object with a cleared environment;
only Windows system-root variables required to start the executable are retained. NDJSON messages,
aggregate stdout, and stderr all have fixed byte limits; request IDs, response count, clean EOF,
successful exit, and bounded cleanup are mandatory. Unix cleanup polls for an empty process group;
Windows cleanup requires a successful Job-wide termination request plus leader and I/O completion.

The core verifies fixed item budgets and the exact canonical file-id/path set against `SourceMap`,
and converts every raw UTF-8 range into an opaque, map-bound `Span`. The driver-facing API returns
only the resulting verified project. Raw provider bytes and DTOs do not cross that boundary.

`native_lexer` is the first internal native-frontend stage. Its `admit_and_lex` boundary validates
portable paths and fixed raw-byte budgets before strict UTF-8 conversion, then owns the resulting
`SourceMap` together with the bound lexical project. Malformed encoding uses an exact
pre-authority `RawByteSpan`; it is never represented as a forged source-map `Span`. The existing
`lex(&SourceMap)` entry remains available unchanged for already-authenticated sources. Both paths
walk canonical `SourceMap` files,
retains a lossless ordered stream of tokens and whitespace/comment trivia, and issues only
source-map-authenticated UTF-8 spans. Its ASCII identifier boundary and protocol-v4 punctuation,
keyword, decimal, and unescaped string inventory are deterministic; malformed scalars, strings,
and comments recover at character boundaries with stable diagnostics. Fixed token, trivia,
project, diagnostic, and protocol-v4 aggregate-source budgets fail atomically as `ZRYNA-F1502`;
recoverable malformed input and invalid UTF-8 admission are reported as `ZRYNA-F1501`. Raw bytes
are never normalized or repaired. Identifiers are ASCII and at most 128 bytes; `constructor`,
`prototype`, and `__proto__` are forbidden at lexical admission. Strings are single- or double-quoted with
no escapes or line terminators, and `//` and `/* ... */` comments remain lossless trivia. Whitespace uses the pinned TypeScript 6 scanner's exact set,
including U+0085, U+200B, and U+FEFF; CR, LF, U+2028, and U+2029 terminate line comments.
U+180E, U+2060, and U+001C remain lexical F1501 failures. Those pre-parser failures are a
distinct native lexical admission contract, not equivalent bootstrap parser diagnostics. The
lexical inventory covers the v4 keywords plus braces, brackets, parentheses, `: ; , .`, `< <= >
>=`, `= => === !==`, and `+ - *`. Per-file token and trivia limits are 65,536 each; the project
retains at most 262,144 combined lexemes, 256 diagnostics, and 8 MiB of source. This stage does not
parse, create protocol-v4 snapshots, implement a provider, or change bootstrap/public selection.
Run the `native_lexer`, `native_lexer_admission`, and `native_lexer_fuzz` integration tests for the
ordinary corpus. The routed provider-v4 lane runs a direct exact-span differential against the
pinned TypeScript 6 provider on Linux and Windows. Add `-- --include-ignored` to execute the three
proportional production-limit token, trivia, project-lexeme, and raw-byte proofs without lowering
their limits.

`native_parser::parse_v2_candidate` constructs an internal untrusted protocol-v2 DTO over
that exact source-bound stream. The admitted M1 grammar consists of exported functions with
named or missing parameter/result annotations and returns containing ASCII references, Boolean
literals, canonical decimal integers, and left-associative addition. Files, functions, and
statements retain source order; expressions use canonical postorder arenas and exact UTF-8
spans. Trailing parameter commas and semicolon omission at a closing brace or before a
line-separated return follow the pinned worker. A line terminator directly after `return`
produces separate unsupported return and expression-statement diagnostics.

The separate `parse_v2_recovering_candidate` mirrors the frozen worker's recovery boundary.
TypeScript parse diagnostics suppress normalization for that file. Well-formed rejected
functions retain bounded source diagnostics, discard their function DTO, and permit later valid
functions. Rejected declarations and statements consume balanced source nodes; signature errors
retain the parameter/function ordinal and whole annotation or initializer node span. Rejected
returns roll back their temporary expression arena and project expression budget, while rejected
functions still count toward function, parameter, and statement budgets. This candidate requires
`zryna_syntax::v2::verify_snapshot`; retained error diagnostics prevent semantic input.

The recovery collector keeps the earliest 255 diagnostics in canonical file/span/code/message
order and emits one global `ZRYNA-F2003` truncation diagnostic when another diagnostic occurs.
Production inventory exhaustion is a distinct atomic `ZRYNA-F1002` rejection. Lexical failure,
foreign source maps, and fatal resource failures never expose a partial candidate. Parentheses,
calls, multiplication, strings, primitive or compound annotations, nonexported/generic/generator
functions, and non-M1 declarations remain unsupported; frozen cases verify their complete
rejected-node diagnostics and retained sibling functions. Unsupported TypeScript syntax beyond
the frozen rejection corpus is excluded from parser equivalence.

`tests/native_parser_parity.rs` compares exact raw snapshots, diagnostic multiplicity, codes,
messages, guidance, and spans against pinned receipts, then applies the existing versioned
verifiers to every candidate. Its ordinary and depth corpora cover mixed recovery, malformed
source, whitespace and line-terminator variants, signed literals, precedence, callback and
ownership rejection, and diagnostic/depth ordering. Production boundary sources and first-extra
receipts live beside those corpora. The live lane requires both the frozen-receipt comparison
and production-limit comparison; deterministic token-grammar mutations separately prove bounded,
panic-free rejection or verifier acceptance.

`native_parser::v3::parse_v3_import_candidate` is a separate internal M2 candidate for
functionless modules containing named imports only. It consumes every nontrivia native token or
rejects the complete project, so a later function, declaration, malformed import, or trailing
token cannot leave an omitted import in a returned snapshot. The admitted subset preserves source
order, aliases, plain names, single- or double-quoted explicit relative `.zry` specifiers, and
exact UTF-8 keyword, binding, token, and value spans. Import and binding inventories fail on the
first extra item within the v3 limits. A frozen TypeScript 6 protocol-v3 import-only snapshot and
the earlier complete v3 worker fixture provide differential evidence; the existing
`zryna_syntax::v3::verify_snapshot` remains the only syntax authority. This candidate does not
parse functions or other M2 declarations and is not registered as a frontend provider. It does
not resolve modules, admit a public profile, or change v2, v3, or v4 protocol contracts.

`native_parser::v3::parse_v3_straight_line_candidate` separately admits a named-import prefix
followed by source-ordered exported or unexported functions. The function-body parser admits typed
`let` or `const` declarations, simple `name = expression;` assignments, value returns, standalone
blocks, braced `if` with an optional braced `else`, and braced `while`, including empty and nested
blocks. Parameter and result annotations may be missing in the syntax DTO; named annotations
remain source-faithful for later semantic validation. Initializers, assignment values, conditions,
and returns use identifiers, Boolean literals, canonical nonnegative integers,
compact canonical negative decimal integers through 64 total bytes (one literal node), numeric
negation of one canonical nonnegative decimal token of at most 64 bytes in other cases (two nodes),
direct identifier calls with up to 256 source-ordered arguments, and unary minus over
nonparenthesized atoms. Binary operators are `*`, `+`, `-`, `<`, `<=`, `>`, `>=`, `===`, and `!==`,
with the pinned worker's precedence and left associativity. Trailing call-argument and parameter
commas are accepted. It emits preorder block and statement arenas with source-ordered postorder
expressions.
Frozen two-file TypeScript 6 v3 function, local, assignment, zero-argument call, signed-literal,
identifier-negation, numeric-negation, scalar-expression, and lexical-block snapshots, the complete
two-file M2 straight-line syntax snapshot, the positive and semantic-negative M2 control-flow
snapshots, the complete two-file v3 snapshot, and UTF-8/CRLF and type-syntax snapshots are exact DTO
oracles; the v3
verifier remains authoritative. Syntax acceptance of a name before its declaration, assignment to a
`const`, or call to an unresolved function gives it no semantic authority. Every nontrivia token
must be consumed, including after a valid function, or the complete candidate fails. Function,
parameter, block, statement, and expression boundaries have exact and first-extra tests. Source
delimiter nesting and statement-context expression depth follow the pinned worker's 128-depth
boundary, including its flat-addition 127/128 split. The original import-only entry retains its
closed behavior for source closure. A live pinned-worker test compares all 14 public M2 source
files (13 exact candidates and one equivalent rejection), six further rejected forms, and 128
deterministic accepted or atomic-rejection grammar mutations. Compound or property assignment,
parenthesized expressions or callees, indirect, generic, optional, or spread calls, other
operators, provider selection, and module resolution remain
outside this parser slice.

`native_parser::v4::parse_v4_candidate` constructs an untrusted protocol-v4 candidate from the
same bound lexical stream. It admits named imports; source-ordered struct and enum declarations;
module-wide postorder type syntax; functions with preorder blocks and statements; scalar and
control-flow syntax; and the documented data, collection, ownership, match, and weak-upgrade
forms. It enforces source, declaration, type, function, block, statement, expression, local,
aggregate, and match-arm limits before exposing a candidate. Frozen TypeScript 6 snapshots cover
UTF-8 and CRLF spans, interleaved declarations, two-file order, nested type construction, and the
new expression and statement forms. Generated expression mutations are also checked by the
existing `zryna_syntax::v4::verify_snapshot`, which remains the only syntax authority. The
`provider:conformance:v4` gate compares native candidates with the pinned TypeScript 6 worker for
all 95 frozen M3 source fixtures, one four-file composition, and seven rejected forms. It requires
identical raw candidates and exact rejection codes, messages, and spans. The entry does not register a provider, perform semantic checks, resolve imports, or
activate a public profile. Protocol-v3/v4 unsupported source fails atomically, matching the
bootstrap worker; later-function recovery belongs to protocol v2. The shared parity corpus
checks complete rejection nodes, malformed-source priority, and expression-depth diagnostics
before candidate inventory allocation for long scalar, call, construction, field, and index
chains. A lexer-bounded source-shape pass follows expression preorder depth checks and the
worker's inventory reservation order using copied current counters. Struct operands reserve
before their field values; array and enum operands reserve after their values. Invalid match
arms retain the earlier scrutinee and arm-value checks. The pass does not allocate candidate
syntax or consume real parser budgets. Frozen priority cases include populated function arenas
and exhausted project aggregate budgets, with exact nonzero-file spans.

Production resource fixtures compare all 63 per-item boundaries and 24 reachable project
boundaries, including valid limits and first-extra rejections. Separate receipts preserve the
statement-limit priority over the local limit and protocol-v2 rejected-expression rollback.
Some parser ceilings cannot be reached through native lexical admission: project parameter,
expression, local, match-arm, import, and member inventories, and module/project type arenas,
require more lexemes than the fixed native lexer permits. Those lexer-first cases are exclusions
from parser diagnostic equivalence; parser and lexer limits remain unchanged.

Whole parameter, named-import binding, ordinary call-argument, data-member, construction-field,
array-element, and match-arm counts are checked before normalizing their children, following the
worker's order. Function, import, block, statement, and local reservations remain interleaved with
source traversal. Recognized type constructors check argument arity before descending, and
specialized call or weak-upgrade callback shapes are checked before their nested inventories.
The per-function local ceiling is dominated by the statement ceiling, whose diagnostic wins on
the first extra local declaration.

Known malformed-source diagnostics are checked after global source nesting and before any
normalization or inventory reservations within each file. A parse error therefore precedes a
function or parameter limit in the same file; an earlier file's fatal inventory failure still
precedes a later file's parse error. Rejected type annotations retain their parameter, result,
local, data-member, or array-element context. Parameter ordinals follow delimiter-aware ranges,
including commas inside earlier generic annotations.
For a generic local annotation immediately followed by `=`, the parser retains separate `>`
and `=` source ranges from the lexer's single `>=` token. Ordinary comparison tokens are unchanged.

Protocol v1 intentionally carries declarations and diagnostics only. Protocol v2 is a separate
executable-syntax contract owned by `zryna-syntax`; it does not change v1 semantics in place. The
TypeScript 6 adapter implements the protocol-v2 executable-syntax contract. Protocol v3 has its own
syntax-only worker and source-map-verifying transport, including the exact
`control_flow_v1: true`, `module_resolution: false`, and `semantic_diagnostics: false`
capabilities. It is not connected to the driver, semantics, backends, or CLI.
