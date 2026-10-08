# Zryna architecture engine

`zryna-architecture` proves that a repository can be inspected completely and matches
its authoritative workspace contract. It fails closed on unsafe files, incomplete
inspection, stale Cargo inputs, undeclared dependencies and forbidden phase edges.

The stable public entrypoint is `validate_workspace(&Path) -> ValidationReport`.
`WorkspaceContract`, `MemberContract`, `MemberKind`, `AdapterContract` and
`ValidationReport` retain their existing fields, derives and serialization contracts.
The [facade](src/lib.rs) reexports these names; every implementation module is private.

## Responsibility map

| Module | Purpose and entrypoints |
| --- | --- |
| [Validation](src/validation/README.md) | Ordered validation and early stops: `validate_workspace`. |
| [Contract](src/contract/README.md) | Contract models, loading, identity, paths and retained-source comparison. |
| [Filesystem](src/filesystem/README.md) | Portable paths, same-handle bounded reads and globally bounded scanning. |
| [Components](src/components/README.md) | Exact root/component inventory and Rust/adapter manifest checks. |
| [Cargo](src/cargo/README.md) | Input snapshots, bounded metadata capture and actual dependency graph reconciliation. |
| [Dependency graph](src/dependency_graph/README.md) | Permanent phase directions, edge registration and cycle rejection. |
| [Diagnostics](src/diagnostics/README.md) | Validation-wide diagnostic budget and deterministic report ordering. |

Private helpers expose only the interfaces needed by these cooperating modules and
crate-local tests. Validation order, diagnostic messages/codes, limits, Cargo flags,
filesystem exclusions and supported-platform strategies remain owned by this crate.

## Dependencies and verification

Production dependencies are `serde`, `serde_json`, `toml`, `same-file`,
`zryna-diagnostics`, `zryna-process` and Unix-only `libc`. No dependency or component
registration is added by the internal module layout.

The existing unit tests are grouped under [src/tests](src/tests/mod.rs) by boundary:
`paths`, `scan`, `controlled_read`, `cargo_process` and `cargo_graph`. `fixtures.rs`
and `graph_fixtures.rs` provide test-only workspaces and raw metadata. All existing
tests remain crate-local unit tests; platform conditions and assertions are retained.

Use the repository-pinned toolchains and a populated locked Cargo cache:

```sh
cargo fetch --locked
cargo test --locked -p zryna-architecture
cargo clippy --locked -p zryna-architecture --all-targets --all-features -- -D warnings
pnpm structure:check
pnpm preflight
pnpm m0:check
```

The repository validation test uses frozen metadata for the actual checkout. Platform
conditional tests retain their Linux/Windows conditions; a local pass does not replace
required hosted checks. See the repository contribution guide for complete gates.
