# Native syntax candidates

[`native_parser.rs`](../native_parser.rs) is the public library entry for native parsing from a
source-bound `LexedProject`. `parse_v2_candidate` and `parse_v2_recovering_candidate` produce
untrusted protocol-v2 DTOs; the public [v3](v3.rs) and [v4](v4.rs) modules expose their separate
candidate grammars. Native parsing does not select the public compiler frontend.

## Contracts and ownership

Input is one exact `SourceMap` and its bound lexical stream. Output is a source-faithful raw syntax
snapshot or `ParseError`, retaining diagnostics and UTF-8 byte spans. Callers must pass every
candidate through its corresponding `zryna-syntax` verifier before semantic use. The recovering
v2 path discards rejected declarations/functions and resumes at a top-level export; lexical and
resource failures remain fatal. v3/v4 reject unsupported source atomically.

| Private area | Responsibility |
| --- | --- |
| [functions.rs](functions.rs), [expression.rs](expression.rs), [collections.rs](collections.rs) | Closed v2 function/expression grammar and bounded inventories |
| [recovery.rs](recovery.rs), [v2_recovery.rs](v2_recovery.rs), [rejection/](rejection/) | Recovery and bootstrap-compatible rejection priority |
| [depth.rs](depth.rs), [depth/](depth/) | Nesting and source-shape checks before candidate allocation |
| [v3/](v3/) | Import discovery and scalar/control-flow candidates |
| [v4/](v4/) | Data/type declarations and ownership-syntax candidates |

Frontend → source/diagnostics/syntax is the permitted direction. Name resolution, type checking,
layout, raw IR verification, module filesystem discovery, backend emission and CLI activation
belong to their downstream owners. The grammar here is bounded and version-specific; a parsed
construct is not a supported executable feature. See
[native source snapshots](../../../../docs/NATIVE_SOURCE_SNAPSHOTS.md).

## Existing focused tests

Run a version's existing target from the repository root:

```sh
cargo test --locked -p zryna-frontend --test native_parser_v2
cargo test --locked -p zryna-frontend --test native_parser_v3_functions
cargo test --locked -p zryna-frontend --test native_parser_v4_data
```

[v2 fixtures](../../tests/native_parser_v2.rs),
[v3 functions](../../tests/native_parser_v3_functions.rs), and
[v4 data](../../tests/native_parser_v4_data.rs) freeze candidates and spans.
[Parser parity](../../tests/native_parser_parity.rs) checks diagnostic priority;
[v4 corpus](../../tests/native_parser_v4_corpus.rs) checks frozen M3 candidate equality.
