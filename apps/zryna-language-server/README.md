# Zryna language server

Thin, fail-closed stdio transport for compiler-owned protocol-v2 scalar diagnostics/definition
and explicitly selected protocol-v3 M2 or protocol-v4 M3 diagnostics and formatting. The component
owns LSP framing, document lifecycle, revision correlation,
position conversion, cancellation routing, and inert rendering only. `zryna-driver` retains
frontend execution, session scheduling, diagnostic publication, and semantic query authority;
`zryna-source` validates the exact in-memory UTF-8 text and coordinates. The integration tests use
driver-owned fixture support; the transport does not call or configure a frontend provider.

The server accepts `initialize`, `initialized`, `textDocument/didOpen`, full-text
`textDocument/didChange`, `textDocument/didClose`, `textDocument/definition`, `$/cancelRequest`,
`textDocument/formatting`, `textDocument/rangeFormatting`, `shutdown`, and `exit`. It negotiates `utf-8`, `utf-16`, or `utf-32` positions, defaults to
`utf-16`, and accepts only `file:` documents strictly below the initialized root URI. Each source
mutation creates a new immutable compiler revision. Diagnostics are published both as standard
source diagnostics and as an exact `zryna/publishDiagnostics` notification carrying the complete
structured-diagnostics-v2 report and revision pair.

Run the binary with absolute compiler workspace and pinned Node.js 22.22.1 paths:

```text
zryna-language-server --compiler-root <absolute-path> --node <absolute-node-path>
```

Server 0.5.0 also accepts `--installed-root <absolute-verified-compiler-directory>`. It captures
the fixed distribution bootstrap closure, requires worker bytes equal to this build, and verifies
bundled Node/TypeScript hashes before execution. It reuses the same private staging and semantic
authority without a checkout, package manager or runtime override. Initial server authenticity
belongs to the independently verified setup, not adjacent checksums. `--version` reports version,
capability and embedded source revision. See [portable setup](../../docs/PORTABLE_SETUP.md).

The compiler root supplies the fixed registered `adapters/typescript-6/src/worker.mjs`; protocol
messages cannot select an executable, provider identity, filesystem root, network request, build,
or code execution. Without initialize options the server retains one-file protocol-v2 scalar
admission, definition, and `scalar-format-v1`. Exact
`initializationOptions: {"zrynaProfile":"control-flow-v1"}` selects one-file protocol-v3 M2
diagnostics and `control-flow-format-v1`; the initialize response echoes the selected capabilities.
Exact `data-ownership-v1` selects M3 diagnostics and `data-ownership-format-v1`, requiring a
trusted startup `--workspace-root <absolute-path>` matching the initialization root URI. M3
retains a bounded set of open overlays, resolves unopened saved imports through the driver's
authenticated no-follow source session, and revalidates the graph before returning edits. M2/M3
definition is not advertised. Hover, references, rename,
completion, code actions, indexing, debugging, and execution are unsupported.

Each formatting capability accepts only its separately verified profile. See the
[format contract and editor guide](../../docs/LANGUAGE_SERVER.md). Its publication evidence records
the separate Linux/Windows portable acceptance and clean registry installations of editor 0.5.0,
including real Windows extension-host checks against this matching reviewed server.

## Implementation navigation

[src/main.rs](src/main.rs) parses startup options. The public library entries in
[src/lib.rs](src/lib.rs), including `run_stdio` and `Server`, compose bounded stdio with
driver-owned query sessions. Inputs are framed JSON-RPC/LSP messages and full document revisions;
outputs are correlated responses, diagnostics and formatting edits, never applied source writes.

| Private area | Responsibility |
| --- | --- |
| [framing.rs](src/framing.rs), [protocol.rs](src/protocol.rs), [params.rs](src/params.rs) | Bounded framing, message shape and parameter decoding |
| [documents.rs](src/documents.rs), [coordinates.rs](src/coordinates.rs) | Exact document bytes, URI containment and negotiated position encoding |
| [diagnostics.rs](src/diagnostics.rs) | Inert conversion of compiler reports into LSP diagnostics |
| [server.rs](src/server.rs), [server/](src/server/) | Lifecycle, profile selection, queued definition/formatting and outgoing correlation |

Dependency direction is language-server application → driver query sessions/source. Provider
execution, semantic facts, module closure authority and formatting admission remain in the driver.
Build/run requests, editor UI, direct provider configuration and source-write application do not
belong in the transport. See the
[compiler session map](../../crates/zryna-driver/src/diagnostic_sessions/README.md).

Existing focused targets:

```sh
cargo test --locked -p zryna-language-server --test protocol
cargo test --locked -p zryna-language-server --test process
```

[Protocol tests](tests/protocol.rs), including [formatting cases](tests/protocol/formatting.rs),
exercise framing, revisions, cancellation and profile rejection with fixture compiler authority.
[Process tests](tests/process.rs) exercise real stdio and need the repository-pinned frontend host.
