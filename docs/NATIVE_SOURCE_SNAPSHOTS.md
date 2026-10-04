# Native source discovery and snapshots

Status: internal implementation candidate for #413, layered over the #403 source package
contracts and #412 native parser. Required exact-revision Linux and Windows evidence remains
necessary before acceptance. Public frontend selection stays on the existing bootstrap route.

## Ownership and lifecycle

`zryna-driver` owns `capture_native_workspace_sources` and `capture_native_package_sources`.
The workspace route borrows a retained `WorkspaceSourceRoot`; the package route retains the
frozen resolver's complete capability set and selects one exact manifest identity within that
authenticated graph. Neither route receives authority from a provider-selected host path.

Discovery reads each reached workspace source exactly once through a retained no-follow regular
file handle. Package discovery consumes the exact immutable bytes read once by package resolution.
Directory capabilities and bounded inventories remain retained. Final source content is never
reopened or reread for authentication, parsing, semantics or target dispatch.

The native lexer and the import-discovery helper see original text and original offsets. The
helper extracts untrusted top-level named-import candidates while skipping balanced function/data
bodies. It produces no executable syntax authority and cannot suppress the complete parser's
later rejection. Explicit relative `.zry` resolution stays in the driver. Reachable paths are
deduplicated before reading, case-fold collisions reject, and repeated binding edges and module
cycles reject before complete syntax parsing.

`NativeSourceSnapshot` seals sorted canonical paths, dense module IDs, exact raw source hashes,
source-derived import edges, both existing graph digests and one original `SourceMap`. The same
issuing map identity and exact UTF-8 text are retained through `verify_v2`, `verify_v3` or
`verify_v4`. The complete native parser produces an untrusted candidate; the existing versioned
syntax verifier authenticates it. For v3/v4, final source-map-bound edges must exactly equal the
presealed graph. Omitting an import, changing its names, spans, target or graph identity cannot
produce an accepted closure.

The returned native syntax/module/ownership snapshots retain the source owner beside the verified
syntax or existing driver closure. Callers keep this owner alive and revalidate before target
dispatch. Semantics and the mandatory IR verifier remain the only route to backend authority.
No raw graph or syntax DTO can substitute for verified IR.

## Identity and packages

Native module digests use the existing canonical `ZRYNA-M2-GRAPH` and `ZRYNA-M3-GRAPH` domains,
version 1, little-endian length-prefixed UTF-8 fields, sorted paths with raw SHA-256 and canonical
named-binding edges. One shared serializer owns both frozen byte layouts. Absolute host paths,
capture order and the ambient working directory do not enter either digest.

Package snapshots additionally retain the #403 exact manifest/source identity pair and the
canonical lock digest. Capture requires frozen resolution and performs no lock update. A chosen
dependency instance must already occur in that authenticated graph. Relative imports stay within
that instance's declared inventory. The current package schema has no exported-module table:
cross-package alias imports, deep imports and package type imports therefore remain rejected.

## Bounds and rejection

The existing source, lexer, module and package limits remain unchanged. Workspace closures admit
at most 4,096 files, 2 MiB per file and 8 MiB aggregate bytes; canonical import declarations,
binding edges and conservative manifest-byte accounting retain their existing ceilings. Native
lexical and versioned syntax limits may reject before a larger graph ceiling is reachable.
Package inventories retain the stricter 16 files/1,024 bytes per file and complete package graph
limits. First-extra items fail without exposing partial syntax or compiler authority.

Retained-state checks compare safe no-follow metadata against the exact live source handle.
Unix checks device/inode, link count, size, modification and change timestamps; Windows source
handles deny write/delete sharing throughout retention and checks reject reparse attributes and
changed state. Directory identity and inventory revalidation remain capability-relative. An
observed source, parent or root substitution rejects before verification/dispatch. Original
snapshot bytes remain immutable even when a Unix writer subsequently alters the live filesystem.
These checks are not an operating-system sandbox against an arbitrary hostile writer; they
authenticate the observations at the checked boundaries, while compilation consumes only the
previously sealed immutable bytes.

| Diagnostic | Rejection |
| --- | --- |
| `ZRYNA-D3001` | Non-admitted entry or relative import path/escape. |
| `ZRYNA-D3002` | Unsafe retained source/root, link or duplicate physical source identity. |
| `ZRYNA-D3003` | Missing/unreadable regular source or invalid UTF-8. |
| `ZRYNA-D3004` | Observed source/root/directory substitution or stale retained state. |
| `ZRYNA-D3005` | Exact-case mismatch or portable case collision. |
| `ZRYNA-D3006` | Duplicate named binding edge. |
| `ZRYNA-D3102` | Source/hash/import/graph differs from its sealed authority. |
| `ZRYNA-D3201` | Native source/graph accounting or discovery deadline exceeded. |
| `ZRYNA-D3301` | Existing ownership graph cycle rejection. |
| `ZRYNA-P4004` | Admitted package inventory, source hash or retained capability mismatch. |
| `ZRYNA-P4006` | Requested exact package identity absent from the admitted graph. |
| `ZRYNA-P4010` | Non-frozen native capture or stale frozen package lock. |

Existing lexer, native parser and syntax-verifier diagnostics pass through unchanged. V2 recovery
may retain verified error diagnostics, which stop semantic admission. V3/v4 unsupported source
rejects atomically. Package/schema/graph failures keep the existing package diagnostic categories.

## Verification and exclusions

Focused driver tests live under `module_closure::native_sources::tests`. They use complete source
fixtures and independently frozen worker receipts, compare the existing canonical driver graph,
exercise source substitution, link/case/path attacks, wrong hashes/edges and exact package
selection, and lower genuine verified source to JavaScript/WebAssembly emission while retaining
the same source. Platform-specific concurrency tests cover Unix stale bindings and Windows
sharing/junction rejection. Proportional source-file, aggregate-byte, retained-file-count and
named-binding-edge tests are ignored in ordinary runs and require `--include-ignored` in complete
verification.

The Windows Rust CI job runs `node scripts/run-native-source-resource-tests.mjs` after the ordinary
driver suite. This invokes the four ignored resource cases serially and requires each exact test
name to pass, with four passed and zero failed/ignored; empty or partial output cannot qualify.

Semantic name resolution, version solving, network acquisition/registries, watch mode, package
export syntax, public provider selection, new profiles and frontend-owned filesystem access are
excluded. Focused checks do not waive preflight, complete affected gates, or Linux/Windows merge
requirements.
