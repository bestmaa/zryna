# Compiler query sessions

This feature retains immutable source revisions and compiler-owned diagnostics, scalar definition
facts, and profile-specific formatting plans for tooling transports. The public library entry is
[`diagnostic_sessions`](../diagnostic_sessions.rs): `DiagnosticSession` admits revisions and
correlates bounded requests; `ToolingCompiler` captures and invokes the pinned frontend.

## Contracts and ownership

Inputs are exact `SourceMap` authority, verified syntax or diagnostic records, and bounded query
messages carrying a session-issued snapshot and revision. Outputs are correlated query responses,
structured-diagnostics-v2 reports, definition locations, or presentation-only formatting edits.
Foreign authority, stale revisions, cancellation, deadlines, and exhausted budgets do not produce
invented semantic facts. M2/M3 admission supplies diagnostics and formatting, not scalar definition
facts. See the [language-server contract](../../../../docs/LANGUAGE_SERVER.md).

| Private area | Responsibility |
| --- | --- |
| [request.rs](request.rs), [response.rs](response.rs) | Bounded decoding and correlated rendering |
| [retention.rs](retention.rs), [scheduling.rs](scheduling.rs) | Cache charges, revision lifetime, cancellation and deadlines |
| [semantic_definition.rs](semantic_definition.rs) | Queries over semantics-owned definition authority |
| [formatting/](formatting/) | Edits derived from admitted syntax and exact source bytes |
| [tooling_compiler.rs](tooling_compiler.rs), [tooling_execution/](tooling_execution/) | Captured provider execution and source admission |

Dependency direction is transport → driver sessions → frontend, syntax authority, semantics and
source/diagnostics. The transport must not reconstruct semantic records or select worker bytes
through a protocol message. Build/run dispatch, application of source edits, LSP framing, and editor
UI belong outside this module.

## Existing focused tests

From the repository root:

```sh
cargo test --locked -p zryna-driver diagnostic_sessions::tests
```

The [test root](tests.rs) routes to [bounds](tests/bounds.rs),
[hostile requests](tests/hostile.rs), [lifecycle](tests/lifecycle.rs),
[definition](tests/definition.rs), [M2 admission](tests/control_flow.rs), and
[formatting](tests/formatting.rs). Transport integration belongs to the
[language-server tests](../../../../apps/zryna-language-server/tests/protocol.rs).
