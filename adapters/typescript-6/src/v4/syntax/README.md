# Source-faithful syntax normalization

`source.mjs:normalizeSource()` parses one admitted source and emits the existing
protocol-v4 file snapshot. Dispatch supplies the file's sorted ID, collector, and
shared project counters. The TypeScript API is accessed through boundary
configuration, which verifies its exact version once before normalization.

| File | Responsibility and entrypoints | Internal dependencies | Focused v4 tests |
| --- | --- | --- | --- |
| `source.mjs` | Source byte/UTF-16/hashbang checks, parse diagnostics, source-order top-level traversal. | Tokens, spans, diagnostics, imports, data declarations, functions, boundary limits/errors. | UTF-8 CRLF fixtures, malformed source, atomic sibling rejection, source/project limits. |
| `spans.mjs` | `buildUtf8OffsetMap()`, `spanFromOffsets()`, `nodeSpan()` translate exact UTF-16 offsets to UTF-8 spans. | Boundary invariant errors. | Astral Unicode and token/source spans. |
| `tokens.mjs` | `findToken()`, `requiredToken()`, `enforceParserNesting()` retain token discovery and scanner nesting checks. | TypeScript, limits/errors. | Required punctuation, parser depth exact/plus-one. |
| `names.mjs` | `normalizedIdentifier()` and `dataName()` reject escaped/noncanonical/defensive names. | Spans and identifier limit. | Hostile identifiers, fields, variants and bindings. |
| `diagnostics.mjs` | `DiagnosticCollector` and `compactText()` retain located errors, deterministic ordering/truncation, bounded text. | TypeScript, spans, limits/errors. | Located unsupported syntax, deterministic failures and bounded messages. |
| `types.mjs` | `normalizeType()` and `pushType()` build dense type syntax in canonical order. | Names, tokens, spans, limits/errors. | Ownership/container types, fixed-array lengths, type arena/nesting budgets. |
| `imports.mjs` | `normalizeImport()` records source-faithful named imports and module-specifier spans. | Names, tokens, spans, limits/errors. | Import rejection, aliases, binding limits; no resolution. |
| `data-declarations.mjs` | `normalizeDataDeclaration()` records nominal struct/enum syntax and members. | Names, types, tokens, spans, limits/errors. | Data declarations, members, nominal declaration budgets. |
| `functions.mjs` | `normalizeFunction()` owns signatures and per-function body context; private parameter normalization. | Names, types, statements, tokens, spans, limits/errors. | Signatures, ownership annotations, parameter/function budgets. |
| `statements.mjs` | `allocateBlock()` and private statement normalization preserve preorder blocks and source-order statements. | Expressions, names, types, tokens, spans, limits/errors. | Locals/assignment/control flow, semicolons, block/statement/local budgets. |
| `expression-arena.mjs` | `pushExpression()` and `countAggregateOperands()` allocate postorder expression IDs and enforce aggregate counters. | Limits/budget errors. | Expression/project and construction operand limits. |
| `expressions.mjs` | `normalizeExpression()` dispatches scalar/data operations in their existing recognition order. | Arena, constructions, matches, names, tokens, spans, limits/errors. | Scalar precedence, ownership operations, exact operator inventory, depth/call limits. |
| `constructions.mjs` | Normalize struct fields and typed arrays; call the supplied expression normalizer for operands. | Arena, names, types, tokens, spans, limits/errors. | Shorthand fixture bytes, struct/array construction, construction budgets. |
| `matches.mjs` | `normalizeMatchExpression()` records source-order enum arms and their bindings/values. | Arena, names, tokens, spans, limits/errors, supplied expression normalizer. | Match grammar and per-expression/project arm limits. |

Normalizers report syntax only. Unsupported or recovered syntax fails the entire
request; no supported siblings are silently returned. Preserve recognition and
counter ordering when changing helpers: it affects the first error and wire bytes.
Keep expression recursion explicit through callbacks in construction/match helpers;
the module import graph is acyclic.

Run `pnpm --filter @zryna/adapter-typescript-6 test:v4` and
`node --test tests/syntax-protocol-v4.test.mjs` from the repository root.
The Rust ingestion boundary is tested by
`cargo test --locked -p zryna-frontend --test worker_process`; it remains separate
from this provider's syntax construction.
