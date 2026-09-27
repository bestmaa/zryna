# Zryna frontend contract

Versioned, provider-neutral boundary for replaceable TypeScript and native Zryna frontends.

Provider output is untrusted. Protocol-v1 adapters retain their declaration-only legacy contract.
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
are never normalized or repaired. Identifiers are ASCII and at most 128 bytes, strings are single- or double-quoted with
no escapes or line terminators, and `//` and `/* ... */` comments remain lossless trivia. The
lexical inventory covers the v4 keywords plus braces, brackets, parentheses, `: ; , .`, `< <= >
>=`, `= => === !==`, and `+ - *`. Per-file token and trivia limits are 65,536 each; the project
retains at most 262,144 combined lexemes, 256 diagnostics, and 8 MiB of source. This stage does not
parse, create protocol-v4 snapshots, implement a provider, or change bootstrap/public selection.
Run the `native_lexer`, `native_lexer_admission`, and `native_lexer_fuzz` integration tests for the
ordinary corpus. The routed provider-v4 lane runs a direct exact-span differential against the
pinned TypeScript 6 provider on Linux and Windows. Add `-- --include-ignored` to execute the three
proportional production-limit token, trivia, project-lexeme, and raw-byte proofs without lowering
their limits.

`native_parser::parse_v2_candidate` is an internal first parser slice over that exact bound token
stream. It constructs untrusted protocol-v2 DTOs for exported functions with named or missing
parameter/result annotations and return statements containing ASCII
references, Boolean literals, canonical decimal integers, and left-associative addition. It
retains source-order files/functions/statements, exact token-based UTF-8 spans, and canonical
postorder expression arenas. The frozen M1 subset includes trailing parameter commas and
semicolon omission at a closing brace or before a line-separated return; a line break directly
after `return` remains rejected. First-extra v2 inventory and depth failures are atomic. Lexical diagnostics,
foreign source maps, malformed input, and syntax outside this closed slice are rejected. The
existing `zryna_syntax::v2::verify_snapshot` remains the only syntax authority. This partial
slice does not cover parenthesized expressions, M2/M3 grammar, bootstrap-equivalent recovery
diagnostics, or provider selection;
it is not a completed native frontend. `tests/native_parser_v2.rs` compares the frozen bootstrap
M1 snapshot and checks verifier acceptance plus negative/resource cases.
The separate `parse_v2_recovering_candidate` synchronizes at the next top-level `export` after a
rejected declaration and balanced braces, brackets, and parentheses, retaining later valid
functions and bounded errors. Mismatched delimiters stop synchronization. It never returns a
partial candidate on lexical or resource failure. Its DTO still requires the v2 verifier; any
retained error blocks semantic input. The frozen parenthesized-return and return-newline cases
match bootstrap diagnostic spans, wording, guidance, and multiplicity. Rejected direct calls,
simple multiplication chains, and string literals now have the same exact subtree spans and
diagnostic text as the bootstrap provider. Unsupported primitive parameter and result annotations
now preserve the pinned bootstrap keyword span, function or parameter index, diagnostic text, and
guidance for one unsupported primitive annotation per rejected function. A distinct named type,
such as `String`, remains a candidate for semantic checking. A balanced call is scanned iteratively
over the lexer's bounded token stream; recovery still resumes only at a genuine top-level export. Other
unsupported annotation forms, multiple errors in one function, expressions, and declarations still
need independent differential recovery evidence.

| Checked M1 source set | Native candidate evidence | Remaining gap |
| --- | --- | --- |
| `examples/universal/add.zry`, `tests/m1-fixtures/{bool-gated,invalid-any}.zry` | Exact frozen bootstrap DTO and v2 verifier acceptance | Semantic outcomes remain owned by the existing compiler. |
| Missing annotations, trailing parameter comma, semicolon omission, and UTF-8 comment prefix | Exact frozen bootstrap DTO and v2 verifier acceptance | Broader TypeScript syntax is excluded. |
| Newline directly after `return` followed by `1;` | Both frozen bootstrap F2002 diagnostics and verified error snapshot | Other expression-statement forms remain unproven. |
| Unsupported parenthesized return before a valid function | Exact frozen bootstrap diagnostic and retained function; verified error snapshot | Parenthesized syntax remains unsupported. |
| Unsupported direct calls, simple multiplication, and string literals with a later valid function | Exact frozen bootstrap diagnostics and retained function; verified error snapshot | Other unsupported expressions retain generic rejection. |
| Unsupported primitive annotations with a later named-type function | Exact frozen bootstrap diagnostics and retained function; verified error snapshot | Multiple errors in one function and other unsupported type forms remain unproven. |
| First-extra functions, parameters, expression depth, and recovery diagnostics | Bounded focused tests | Full resource and fuzz corpus remains pending. |

Protocol v1 intentionally carries declarations and diagnostics only. Protocol v2 is a separate
executable-syntax contract owned by `zryna-syntax`; it does not change v1 semantics in place. The
TypeScript 6 adapter implements the protocol-v2 executable-syntax contract. Protocol v3 has its own
syntax-only worker and source-map-verifying transport, including the exact
`control_flow_v1: true`, `module_resolution: false`, and `semantic_diagnostics: false`
capabilities. It is not connected to the driver, semantics, backends, or CLI.
