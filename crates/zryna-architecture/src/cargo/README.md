# Actual Cargo dependency inspection

`validate_resolved_cargo_graph` reconciles the actual Cargo workspace and graph with
the authoritative registration. Declared dependencies and resolved edges both count;
normal, development, build, target, renamed and optional dependencies retain their
existing enforcement.

| File | Responsibility and entrypoints |
| --- | --- |
| `mod.rs` | Registered canonical roots, internal package mapping, workspace membership and graph orchestration. |
| `inputs.rs` | Required/optional input snapshots and `validate_cargo_inputs_unchanged`, including absence and case spelling. |
| `metadata.rs` | Private metadata wire models and `validate_cargo_metadata_limits`. |
| `process.rs` | `load_cargo_metadata`, exact Cargo arguments, child lifecycle and the shared deadline. |
| `capture.rs` | Bounded stdout/stderr readers, deadline-bound receipt and metadata output validation. |
| `edges.rs` | Declared/resolved edge collection and exact contract/actual graph comparison. |

Dependencies: `zryna-process`, standard process/thread/channel/I/O, Serde/JSON,
filesystem identity/read helpers, component manifest limits, contract models and shared
diagnostics. Production metadata retains `--frozen`, `--all-features` and format v1.
Output, package, edge and duration limits are unchanged; reader deadlines remain tied
to the process deadline. Input snapshots are revalidated after graph inspection.

Run `cargo fetch --locked` then `cargo test --locked -p zryna-architecture`.
`src/tests/cargo_process.rs` covers process/output/metadata limits and optional input
changes, including the Unix descendant-held-pipe deadline. `src/tests/cargo_graph.rs`
covers aliases, dependency kinds, targets, optional edges and unregistered packages.
`src/tests/graph_fixtures.rs` is test-only raw metadata/workspace construction.
